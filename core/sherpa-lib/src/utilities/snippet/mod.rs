pub mod parse;
use crate::{
    daemon::{
        proto::{event, opt_bool, opt_str, req_str},
        registry::{Cx, MethodGroup},
    },
    error::{CoreError, CoreResult},
    ssh::RemoteExec,
};
use async_trait::async_trait;
use serde_json::{json, Value};
pub async fn execute<E: RemoteExec>(exec: &E, command: &str) -> CoreResult<Value> {
    let start = std::time::Instant::now();
    let out = exec.exec(&format!("LC_ALL=C; export LC_ALL;\n{command}")).await?;
    Ok(parse::result(&out, start.elapsed().as_millis()))
}
fn targets(cx: &Cx, p: &Value) -> CoreResult<Vec<String>> {
    let mut ids = if let Some(a) = p.get("targets") {
        let a = a
            .as_array()
            .ok_or_else(|| CoreError::invalid("targets", "expected array"))?;
        a.iter()
            .map(|v| {
                v.as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| CoreError::invalid("targets", "expected connection IDs"))
            })
            .collect::<CoreResult<Vec<_>>>()?
    } else if let Some(id) = opt_str(p, "connection_id")? {
        vec![id]
    } else {
        let group = opt_str(p, "group_id")?;
        if group.is_none() && !opt_bool(p, "all")? {
            return Err(CoreError::required("targets"));
        }
        cx.state.with_store(|s| {
            let mut stmt = s
                .conn
                .prepare("SELECT id FROM connections WHERE (?1 IS NULL OR group_id=?1) ORDER BY id")?;
            let rows = stmt.query_map([group], |r| r.get::<_, String>(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })?
    };
    ids.sort();
    ids.dedup();
    if ids.is_empty() || ids.len() > 1000 {
        return Err(CoreError::invalid("targets", "expected 1 to 1000 targets"));
    }
    Ok(ids)
}
pub struct SnippetMethods;
pub struct MultiMethods;
#[async_trait]
impl MethodGroup for SnippetMethods {
    fn prefix(&self) -> &'static str {
        "snippet"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "snippet.list",
            "snippet.get",
            "snippet.set",
            "snippet.del",
            "snippet.run",
            "snippet.history",
        ]
    }
    async fn call(&self, cx: &Cx, m: &str, p: Value) -> CoreResult<Value> {
        call(cx, m, p).await
    }
}
#[async_trait]
impl MethodGroup for MultiMethods {
    fn prefix(&self) -> &'static str {
        "multi"
    }
    fn methods(&self) -> &'static [&'static str] {
        &["multi.run"]
    }
    async fn call(&self, cx: &Cx, m: &str, p: Value) -> CoreResult<Value> {
        call(cx, m, p).await
    }
}
async fn call(cx: &Cx, m: &str, p: Value) -> CoreResult<Value> {
    match m {
        "snippet.list" => cx.state.with_store(|s| super::notes::list(s, "snippet", None)),
        "snippet.history" => cx.state.with_store(|s| super::notes::list(s, "run", None)),
        "snippet.get" => cx
            .state
            .with_store(|s| super::notes::get(s, "snippet", &req_str(&p, "id")?)),
        "snippet.set" => {
            let id = opt_str(&p, "id")?.unwrap_or_else(crate::util::new_id);
            let tags = p.get("tags").cloned().unwrap_or(json!([]));
            if !tags.as_array().is_some_and(|a| a.iter().all(Value::is_string)) {
                return Err(CoreError::invalid("tags", "expected string array"));
            }
            let v = json!({"id":id,"title":req_str(&p,"title")?,"command":req_str(&p,"command")?,"tags":tags});
            cx.state.with_store(|s| super::notes::put(s, "snippet", &id, &v))?;
            Ok(v)
        }
        "snippet.del" => {
            cx.state.confirmations.consume(cx.client.id, m, &p)?;
            cx.state
                .with_store(|s| super::notes::delete(s, "snippet", &req_str(&p, "id")?))
        }
        _ => {
            let ids = targets(cx, &p)?;
            if ids.len() > 1 && req_str(&p, "confirm_text")? != "выполнить" {
                return Err(CoreError::invalid(
                    "confirm_text",
                    "Type the execution confirmation phrase",
                ));
            }
            let command = if let Some(id) = opt_str(&p, "id")? {
                req_str(
                    &cx.state.with_store(|s| super::notes::get(s, "snippet", &id))?,
                    "command",
                )?
            } else {
                req_str(&p, "command")?
            };
            // Произвольный shell невозможно надёжно классифицировать: подтверждаем каждый запуск.
            cx.state.confirmations.consume(cx.client.id, m, &p)?;
            let run = crate::util::new_id();
            let count = ids.len();
            let cx = cx.clone();
            let task_cx = cx.clone();
            let run_id = run.clone();
            let (ready, started) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(async move {
                if started.await.is_err() {
                    return;
                }
                let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(8));
                let mut tasks = tokio::task::JoinSet::new();
                for id in ids {
                    let cx = task_cx.clone();
                    let run = run_id.clone();
                    let command = command.clone();
                    let permits = permits.clone();
                    tasks.spawn(async move{
 let Ok(_permit)=permits.acquire_owned().await else{return;};
 let result=async{let conn=cx.conn(&id).await?;conn.ensure_writable()?;execute(conn.as_ref(),&command).await}.await;
 let outcome=result.as_ref().map(|v|if v["exit_status"]==0{"ok"}else{"remote.command_failed"}).unwrap_or_else(|e|e.code.as_str());
 let saved=cx.state.with_store(|s|{super::audit::record(s,&cx.client.id.to_string(),Some(&id),"multi.run",outcome)?;let history=json!({"id":crate::util::new_id(),"run":run,"connection_id":id,"timestamp":crate::util::now(),"outcome":outcome});super::notes::put(s,"run",history["id"].as_str().unwrap_or(""),&history) });
 let message=match saved.and(result){Ok(data)=>json!({"run":run,"connection_id":id,"ok":true,"data":data}),Err(error)=>json!({"run":run,"connection_id":id,"ok":false,"error":error})};cx.client.send(&event("multi.result",message)).await;
 });
                }
                let mut failed_tasks = 0;
                while let Some(result) = tasks.join_next().await {
                    if result.is_err() {
                        failed_tasks += 1;
                    }
                }
                task_cx
                    .client
                    .send(&event(
                        "multi.done",
                        json!({"run":run_id,"total":count,"failed_tasks":failed_tasks}),
                    ))
                    .await;
                task_cx.state.subscriptions.lock().expect("subs").remove(&run_id);
            });
            cx.state.subscriptions.lock().expect("subs").insert(
                run.clone(),
                crate::daemon::state::Subscription {
                    client: cx.client.id,
                    kind: "multi",
                    task: task.abort_handle(),
                },
            );
            let _ = ready.send(());
            Ok(json!({"run":run,"total":count}))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[tokio::test]
    async fn captures_exit() {
        let f = FakeExec::new().on("false", "", 1);
        assert_eq!(execute(&f, "false").await.unwrap()["exit_status"], 1);
        assert!(f.calls()[0].starts_with("LC_ALL=C"));
    }
}
