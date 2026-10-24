'use strict';
/* Дом, вкладки, диалоги MVP (форма, импорт, первый запуск, host key), горячие клавиши, самотест */
window.__errs = [];
window.addEventListener('error', e => window.__errs.push(e.message + ' @' + (e.filename || '').split('/').pop() + ':' + e.lineno));
window.addEventListener('unhandledrejection', e => window.__errs.push('promise: ' + e.reason));
const P = new URLSearchParams(location.search);
let activeId = 'home', homeSel = 'web-01', homeGroup = 'Все', homeTag = null, homeQ = '';

/* ---------- вкладки ---------- */
function renderTabs() {
  const t = $('#tabs'); t.replaceChildren(
    h('div.tab' + (activeId === 'home' ? '.active' : ''), { onclick: showHome }, 'Дом'),
    ...[...Desks.values()].map(d => h('div.tab' + (activeId === d.srv.id ? '.active' : ''), { onclick: () => activateTab(d.srv.id) }, dot(d.srv.status), d.srv.name, h('span.x', { onclick: e => { e.stopPropagation(); closeDeskTab(d.srv.id); } }, '×'))),
    h('div.tab.plus', { title: 'Открыть сервер', onclick: showHome }, '+'), h('span.tabs-right', 'Ctrl+Alt+Space — обзор · Ctrl+Space — лаунчер'));
}
function showHome() { activeId = 'home'; $$('.screen').forEach(s => s.classList.remove('active')); $('#home').classList.add('active'); renderTabs(); renderHome(); }
function activateTab(id) { activeId = id; $$('.screen').forEach(s => s.classList.remove('active')); Desks.get(id).el.classList.add('active'); renderTabs(); const d = Desks.get(id); d.renderTaskbar(); }
function openDesk(srv, o = {}) {
  let d = Desks.get(srv.id);
  if (!d) { d = new Desk(srv, { fast: P.has('fast') }); Desks.set(srv.id, d); $('#screens').append(d.el); }
  activateTab(srv.id);
  if (o.app) { const run = () => d.open(o.app, o.opts || {}); d.connEl.style.display === 'none' ? run() : d.onReady = run; }
  return d;
}
function closeDeskTab(id) { const d = Desks.get(id); if (!d) return; d.destroy(); Desks.delete(id); if (activeId === id) showHome(); else renderTabs(); }

/* ---------- Дом ---------- */
function filtered() { return SERVERS.filter(s => (homeGroup === 'Все' || s.group === homeGroup) && (!homeTag || s.tags.includes(homeTag)) && (!homeQ || (s.name + s.host + s.tags.join(' ')).toLowerCase().includes(homeQ.toLowerCase()))); }
function renderHome() {
  const home = $('#home'); if (!home.firstChild) { home.append(h('div.homewin', h('div.homebar', h('b', { style: 'font-size:15px' }, 'Sherpa'), h('input.inp.mono.search', { id: 'hq', placeholder: 'Поиск: имя, хост, #тег', oninput: e => { homeQ = e.target.value; renderHome(); } }), h('span.mono.sub', 'Ctrl+F'), h('span.sp'), h('button.btn', { id: 'chkall', onclick: checkAll }, 'Проверить все'), h('button.btn.p', { onclick: () => showConnForm() }, '+ Добавить')), h('div.homebody', h('div.side', { id: 'hside' }), h('div.srvlist', { id: 'hlist' }), h('div.detail', { id: 'hdet' })), h('div.statusbar', { id: 'hstat' }))); }
  const groups = ['Все', ...new Set(SERVERS.map(s => s.group))], tags = [...new Set(SERVERS.flatMap(s => s.tags))].sort();
  $('#hside').replaceChildren(h('div.cap', { style: 'padding:4px 8px' }, 'Группы'), ...groups.map(g => h('div.it' + (homeGroup === g ? '.active' : ''), { onclick: () => { homeGroup = g; renderHome(); } }, g === 'Все' ? 'Все серверы' : [h('span.dot', { style: `--c:${{ Prod: 'var(--gr)', Stage: 'var(--am)', Infra: 'var(--pu)' }[g]}` }), g], h('span.n', g === 'Все' ? SERVERS.length : SERVERS.filter(s => s.group === g).length))), h('div.cap', { style: 'padding:10px 8px 4px' }, 'Теги'), ...tags.map(t => h('div.tag' + (homeTag === t ? '.active' : ''), { onclick: () => { homeTag = homeTag === t ? null : t; renderHome(); } }, '#' + t)), h('span.grow'), h('button.btn.sm', { style: 'border-color:rgba(0,229,255,.4);color:var(--cy)', onclick: () => showImport() }, 'Импорт ~/.ssh/config'), h('button.btn.sm', { style: 'margin-top:4px', onclick: () => showDemoMenu() }, 'Демо-экраны…'));
  const L = filtered(); $('#hlist').replaceChildren(...(L.length ? L.map(s => h('div.srv' + (homeSel === s.id ? '.sel' : ''), { onclick: () => { homeSel = s.id; renderHome(); }, ondblclick: () => openDesk(s) }, h('span.dot.' + s.status + (s.checking ? '.spin' : '')), h('div', h('div.nm', s.name, s.hkChanged ? h('span.chip.rd', { style: 'margin-left:8px' }, 'host key!') : ''), h('div.hs', s.user + '@' + s.host + ':' + s.port)), h('div.r', h('div', { class: { on: 'gr', off: 'am', err: 'rd', unk: 'sub' }[s.status] }, s.checking ? 'проверка…' : STATUS_TXT[s.status] + (s.lat ? ' · ' + s.lat + ' ms' : '')), h('div.sub', '#' + s.tags.join(' #'))))) : [h('div.empty', 'Ничего не найдено', h('div', h('button.btn.sm', { style: 'margin-top:8px', onclick: () => { homeQ = ''; homeTag = null; homeGroup = 'Все'; $('#hq').value = ''; renderHome(); } }, 'Сбросить фильтр')))]));
  const s = SERVERS.find(x => x.id === homeSel); $('#hdet').replaceChildren(...(s ? [
    h('div.row', h('span.dot.' + s.status + (s.checking ? '.spin' : ''), { style: 'width:12px;height:12px' }), h('h2', s.name), h('span.chip.' + { on: 'gr', off: 'am', err: 'rd', unk: '' }[s.status], STATUS_TXT[s.status].toUpperCase()), h('span.sp'), h('button.btn', { onclick: () => openDesk(s, { app: 'terminal' }) }, 'Терминал'), h('button.btn', { onclick: () => check(s) }, 'Проверить'), h('button.btn.p', { onclick: () => openDesk(s) }, 'Рабочий стол →')),
    h('div.mono.sub', { style: 'font-size:11.5px' }, s.status === 'err' || s.status === 'off' ? h('span.rd', s.errText) : 'Проверено ' + s.checked + (s.lat ? ' · задержка ' + s.lat + ' ms' : '')),
    h('div.kv', ...[['host', s.host], ['port', s.port], ['user', s.user], ['auth', s.auth], ['group', s.group], ['tags', '#' + s.tags.join(' #')], ['hostkey', s.hkChanged && !s.hkAccepted ? h('span.rd', s.hk + ' (ИЗМЕНИЛСЯ)') : h('span.cy', s.hk)], ['os', s.os]].map(([k, v]) => h('div', h('b', k), v))),
    h('div.cap', 'Описание'), h('div', s.desc), h('span.grow'),
    h('div.row', h('button.btn', { onclick: () => showConnForm(s) }, 'Изменить'), h('button.btn', { onclick: () => { const c = { ...s, id: s.id + '-copy', name: s.name + '-copy', status: 'unk', checked: 'не проверялся', lat: 0 }; SERVERS.push(c); homeSel = c.id; renderHome(); toast('Создана копия'); } }, 'Дублировать'), h('button.btn.d', { onclick: () => confirmDanger({ title: 'Удалить подключение?', server: s.name, prod: s.group === 'Prod', lines: ['Подключение ' + s.name + ' и его секреты будут удалены из хранилища.'], okText: 'Удалить', onOk: () => { closeDeskTab(s.id); SERVERS.splice(SERVERS.indexOf(s), 1); homeSel = SERVERS[0] && SERVERS[0].id; renderHome(); } }) }, 'Удалить…'))] : [h('div.empty', 'Выберите сервер или добавьте новый', h('div.row', { style: 'justify-content:center;margin-top:10px' }, h('button.btn.p', { onclick: () => showConnForm() }, 'Добавить сервер'), h('button.btn', { onclick: showImport }, 'Импорт из ~/.ssh/config')))]));
  const c = k => SERVERS.filter(x => x.status === k).length; $('#hstat').replaceChildren(dot('on'), h('span', c('on') + ' online'), dot('err'), h('span', c('err') + ' error'), dot('off'), h('span', c('off') + ' offline'), h('span.sp'), h('span', 'core 0.1.0 · key-file OK'));
}
function check(s, quiet) { if (s.checking) return Promise.resolve(); s.checking = true; renderHome(); return new Promise(res => setTimeout(() => { s.checking = false; s.checked = 'только что'; if (s.status !== 'err' && s.status !== 'off') { s.status = 'on'; s.lat = Math.round(rnd(20, 90)); } else if (s.status === 'unk') s.status = 'on'; if (s.status === 'unk') { s.status = 'on'; s.lat = 55; } renderHome(); if (!quiet) toast(s.name + ': ' + (s.status === 'on' ? 'online · ' + s.lat + ' ms' : s.errText || STATUS_TXT[s.status]), s.status !== 'on'); res(); }, P.has('fast') ? 100 : 900 + rnd(0, 600))); }
async function checkAll() { const b = $('#chkall'); b.disabled = true; let n = 0; await Promise.all(SERVERS.map(s => check(s, true).then(() => { n++; $('#hstat').firstChild && ($('#hstat').lastChild.previousSibling.textContent = 'Проверено ' + n + ' из ' + SERVERS.length); }))); b.disabled = false; toast('Проверка завершена'); renderHome(); }

/* ---------- диалоги MVP ---------- */
function hostKeyDialog(s, ok, cancel, first) {
  const changed = !first && s.hkChanged && !s.hkAccepted;
  let m;
  const body = h('div.col', h('div', changed ? [h('b.mono', s.name), ` (${s.host}:${s.port}) предъявил другой host key. Это переустановка сервера либо перехват соединения (MITM). Подключение заблокировано до вашего решения.`] : [`Первое подключение к `, h('b.mono', s.name), `. Сверьте отпечаток ключа сервера и подтвердите, что ему можно доверять.`]),
    h('div.fp', changed ? [h('div.sub', 'БЫЛО   SHA256:Zk3f9aQ1xTq…9QpA   ed25519 · принят 12.10.2026'), h('div.rd', { style: 'font-weight:700' }, 'СТАЛО  ' + s.hk + '   ed25519')] : [h('div.sub', 'ed25519'), h('div.cy', { style: 'font-weight:700' }, s.hk === '—' ? 'SHA256:Qw12Er34Ty…56Ui' : s.hk)]));
  m = modal({ title: changed ? 'Ключ сервера изменился' : 'Новый host key', icon: h('span.dot.' + (changed ? 'err' : 'on')), body, cls: changed ? 'danger' : '', w: 560, noClose: true,
    buttons: changed ? [{ text: 'Принять новый ключ…', cls: 'd', keep: true, fn: () => confirmDanger({ title: 'Принять новый ключ?', server: s.name, prod: s.group === 'Prod', typeName: s.name, okText: 'Принять ключ', lines: ['Старый ключ будет заменён: ' + s.hk, 'Убедитесь, что смена ключа ожидаема (переустановка сервера).'], onOk: () => { s.hkAccepted = true; s.hkChanged = false; m.close(); ok && ok(); renderHome(); } }) }, { text: 'Отмена', cls: 'p', fn: () => cancel && cancel() }] : [{ text: 'Отмена', fn: () => cancel && cancel() }, { text: 'Доверять и подключиться', cls: 'p', fn: () => { if (s.hk === '—') s.hk = 'SHA256:Qw12Er34Ty…56Ui'; ok && ok(); renderHome(); } }] });
  const cb = $$('.btn', m.el).pop(); if (changed) cb.focus(); return m;
}
function showFirstRun() {
  modal({ title: null, w: 560, noClose: true, cls: '', body: h('div.col', { style: 'align-items:center;text-align:center;gap:12px' }, h('div.row', h('span.ico', { style: '--s:44px;--c:var(--acc)' }, 'S'), h('b', { style: 'font-size:30px' }, 'Sherpa')), h('b', { style: 'font-size:16px' }, 'Добро пожаловать'), h('div.sub', 'Секреты серверов шифруются ключ-файлом (AES-256). Он лежит отдельно от базы; без него пароли и ключи не восстановить — сохраните копию.'), h('div.fp', { style: 'width:100%;text-align:left;border-color:rgba(0,229,255,.4)' }, h('span.sub', 'KEY FILE · chmod 600'), h('b.cy', '~/.config/sherpa/key.bin'))),
    buttons: [{ text: 'Импорт из ~/.ssh/config', fn: () => { localStorage.setItem('sherpa.first', 1); setTimeout(showImport, 150); } }, { text: 'Позже', fn: () => { localStorage.setItem('sherpa.first', 1); toast('Напомним о копии ключ-файла позже'); } }, { text: 'Сохранить копию…', cls: 'p', fn: () => { localStorage.setItem('sherpa.first', 1); toast('Копия сохранена: ~/Документы/sherpa-key.bin'); } }] });
}
function showConnForm(s) {
  const edit = !!s; const v = s || { name: '', host: '', port: 22, user: '', group: 'Prod', tags: [], auth: 'key' }; let auth = edit ? (s.auth.startsWith('пароль') ? 0 : 1) : 1;
  const f = k => h('input.inp.mono', { value: v[k] || '', oninput: () => valid() }); const nm = f('name'), host = f('host'), port = h('input.inp.mono', { value: v.port, style: 'width:90px' }), user = f('user'), tags = h('input.inp.mono', { value: v.tags.map(t => '#' + t).join(' ') }), grp = h('select.inp', ['Prod', 'Stage', 'Infra'].map(g => h('option', { selected: g === v.group }, g))), desc = h('input.inp', { value: v.desc || '' });
  const secret = h('div'); const res = h('div.row'); const errs = h('div.rd', { style: 'font-size:12px;min-height:16px' });
  const segEl = h('div.seg', ...['Пароль', 'Ключ: файл', 'Ключ: текст'].map((t, i) => h('button' + (i === auth ? '.on' : ''), { onclick: e => { auth = i; $$('button', segEl).forEach((b, j) => b.classList.toggle('on', j === i)); drawSecret(); } }, t)));
  function drawSecret() { secret.replaceChildren(auth === 0 ? h('label.f', 'Пароль', h('input.inp.mono', { type: 'password', placeholder: edit ? 'не менять' : '' })) : auth === 1 ? h('label.f', 'Файл ключа', h('input.inp.mono', { value: edit ? '~/.ssh/id_ed25519' : '~/.ssh/id_ed25519', style: 'border-color:rgba(0,229,255,.5);color:var(--cy)' })) : h('label.f', 'Ключ (текст)', h('textarea.inp.mono', { rows: 3, placeholder: '-----BEGIN OPENSSH PRIVATE KEY-----' })), auth ? h('label.f', 'Парольная фраза (необязательно)', h('input.inp.mono', { type: 'password', placeholder: edit ? 'не менять' : '' })) : null); }
  function valid() { const e = []; if (!nm.value.trim()) e.push('Укажите имя'); else if (!edit && SERVERS.some(x => x.name === nm.value.trim())) e.push('Имя занято'); if (!host.value.trim()) e.push('Укажите хост'); const p = +port.value; if (!(p >= 1 && p <= 65535)) e.push('Порт 1–65535'); if (!user.value.trim()) e.push('Укажите пользователя'); errs.textContent = e.join(' · '); return !e.length; }
  drawSecret();
  const m = modal({ title: edit ? 'Изменить подключение' : 'Новое подключение', w: 620, body: h('div.col', h('label.f', 'Имя', nm), h('div.row', { style: 'align-items:flex-start' }, h('label.f.grow', 'Хост', host), h('label.f', 'Порт', port)), h('label.f', 'Пользователь', user), h('div.cap', 'Способ входа'), segEl, secret, h('div.row', { style: 'align-items:flex-start' }, h('label.f.grow', 'Группа', grp), h('label.f.grow', 'Теги', tags)), h('label.f', 'Описание', desc), res, errs),
    buttons: [{ text: 'Проверить', keep: true, fn: () => { if (!valid()) return; res.replaceChildren(h('span.dot.spin'), h('span.sub', 'Проверка…')); setTimeout(() => { const bad = /fail|bad/.test(host.value); res.replaceChildren(dot(bad ? 'err' : 'on'), h('span.mono' + (bad ? '.rd' : '.gr'), bad ? 'Хост недоступен: таймаут 10 с' : 'Подключение успешно, ' + Math.round(rnd(25, 60)) + ' ms')); }, 800); } }, { text: 'Отмена' }, { text: 'Сохранить', cls: 'p', keep: true, fn: () => { if (!valid()) return; const o = { name: nm.value.trim(), host: host.value.trim(), port: +port.value, user: user.value.trim(), group: grp.value, tags: tags.value.split(/[\s,#]+/).filter(Boolean), desc: desc.value, auth: auth === 0 ? 'пароль' : 'key · ' + (auth === 1 ? '~/.ssh/id_ed25519' : 'вставлен текстом') };
      if (edit) Object.assign(s, o); else { const n = { id: o.name, status: 'unk', checked: 'не проверялся', lat: 0, hue: '#ffc53d', hk: '—', os: 'Linux', cores: 4, ram: 8, ...o }; SERVERS.push(n); homeSel = n.id; } m.close(); renderHome(); toast(edit ? 'Сохранено' : 'Сервер добавлен'); } }] });
  return m;
}
function showImport() {
  const cfg = [{ n: 'web-01', u: 'deploy', h: '10.0.0.11', p: 22, k: '~/.ssh/id_ed25519' }, { n: 'web-02', u: 'deploy', h: '10.0.0.12', p: 22, k: '~/.ssh/id_ed25519' }, { n: 'db-main', u: 'postgres', h: '10.0.0.20', p: 22, k: '~/.ssh/db_key' }, { n: 'gw-eu', u: 'admin', h: 'gw.example.com', p: 2200, k: '~/.ssh/id_rsa' }, { n: 'build-01', u: 'ci', h: '10.2.0.7', p: 22, k: '~/.ssh/ci_key' }, { n: 'lab', u: 'root', h: '192.168.77.2', p: 22, k: '—' }].map(x => ({ ...x, dup: SERVERS.some(s => s.name === x.n) }));
  cfg.forEach(x => x.on = !x.dup); let step = 0; const wrap = h('div.col');
  function draw() { wrap.replaceChildren();
    if (step === 0) { wrap.append(h('div.sub', `Найдено ${cfg.length} хостов. Шаблоны (Host *, Host web-*) не становятся серверами — их значения применены по умолчанию. Include развёрнут.`), h('div.box', ...cfg.map(x => h('label.row', { style: 'padding:8px 12px;border-top:1px solid rgba(255,255,255,.05);cursor:pointer' }, h('input', { type: 'checkbox', checked: x.on, onchange: e => { x.on = e.target.checked; cnt.textContent = 'Выбрано ' + cfg.filter(y => y.on).length + ' из ' + cfg.length; btnI.textContent = 'Импортировать ' + cfg.filter(y => y.on).length; } }), h('div.grow.mono', h('b', x.n), h('span.sub', ' ' + x.u + '@' + x.h + ':' + x.p + ' · ' + x.k)), h('span.chip.' + (x.dup ? 'am' : 'gr'), x.dup ? 'уже есть — пропуск' : 'новый')))), cnt = h('div.sub.mono', 'Выбрано ' + cfg.filter(y => y.on).length + ' из ' + cfg.length)); }
    else { const made = cfg.filter(x => x.on), skip = cfg.filter(x => !x.on); wrap.append(h('h3', 'Готово'), h('div.row', h('span.chip.gr', 'создано ' + made.length), h('span.chip.am', 'пропущено ' + skip.length), h('span.chip.rd', 'ошибок 0')), h('div.fp', ...made.map(x => h('div.gr', '+ ' + x.n)), ...skip.map(x => h('div.am', '– ' + x.n + (x.dup ? ' (дубликат)' : ' (снято)'))))); } }
  let cnt, btnI;
  const m = modal({ title: 'Импорт из ~/.ssh/config', w: 660, body: wrap, buttons: [{ text: 'Отмена' }, { text: 'Импортировать 3', cls: 'p', keep: true, ref: b => btnI = b, fn: (close, b) => { if (step === 0) { cfg.filter(x => x.on).forEach(x => SERVERS.push({ id: x.n, name: x.n, host: x.h, port: x.p, user: x.u, group: 'Imported', tags: ['ssh-config'], status: 'unk', checked: 'не проверялся', lat: 0, auth: 'key · ' + x.k, desc: 'Импортирован из ~/.ssh/config', hue: '#b388ff', hk: '—', os: 'Linux', cores: 4, ram: 8 })); step = 1; draw(); b.textContent = 'Закрыть'; $$('.btn', m.btns)[0].style.display = 'none'; renderHome(); } else close(); } }] });
  draw(); btnI.textContent = 'Импортировать ' + cfg.filter(y => y.on).length;
}
function showDemoMenu() { const m = modal({ title: 'Демо-экраны', w: 420, body: h('div.col', ...[['Первый запуск', showFirstRun], ['Форма подключения', () => showConnForm()], ['Импорт ~/.ssh/config', showImport], ['Host key: ключ изменился', () => hostKeyDialog(SERVERS[3], () => toast('Ключ принят'), () => toast('Отмена'))], ['Host key: первое подключение', () => hostKeyDialog(SERVERS[4], () => {}, () => {}, true)]].map(([t, f]) => h('button.btn', { onclick: () => { m.close(); setTimeout(f, 60); } }, t))), buttons: [{ text: 'Закрыть' }] }); }

/* ---------- горячие клавиши ---------- */
document.addEventListener('keydown', e => {
  const d = Desks.get(activeId); if (!d) { if ((e.ctrlKey || e.metaKey) && e.key === 'f') { e.preventDefault(); $('#hq') && $('#hq').focus(); } return; }
  const k = e.key.toLowerCase(), c = e.ctrlKey || e.metaKey;
  if (c && e.code === 'Space' && e.altKey) { e.preventDefault(); d.toggleOverview(); }
  else if (c && e.code === 'Space') { e.preventDefault(); d.toggleLauncher(); }
  else if (c && e.altKey && k === 't') { e.preventDefault(); d.open('terminal', { fresh: true }); }
  else if (c && e.altKey && k === 'f') { e.preventDefault(); d.open('files'); }
  else if (c && e.altKey && k === 'n') { e.preventDefault(); d.toggleNotify(); }
  else if (c && e.shiftKey && e.key === 'Escape') { e.preventDefault(); d.open('monitor'); }
  else if (c && k === 'd' && !e.altKey) { e.preventDefault(); d.showDesktop(); }
  else if (c && e.altKey && e.key === 'ArrowRight') { const ids = [...Desks.keys()]; activateTab(ids[(ids.indexOf(activeId) + 1) % ids.length]); }
  else if (e.key === 'Escape') { d.closeLauncher(); d.closeNotify(); d.ovlEl.classList.remove('open'); }
  else if (e.altKey && k === 'tab') { e.preventDefault(); const ws = d.wins; if (ws.length) { const i = ws.findIndex(w => w.active); ws[(i + 1) % ws.length].focus(); } }
}, true);

/* ---------- старт ---------- */
function boot() {
  $('#screens').append(h('div.screen#home'));
  applySettings(); renderTabs(); showHome();
  if (P.has('light')) { document.body.classList.add('light'); }
  const dk = P.get('desk');
  if (dk) { const d = openDesk(SERVERS.find(s => s.id === dk) || SERVERS[0]); const apps = (P.get('apps') || '').split(',').filter(Boolean); const run = () => { apps.forEach(a => { const [id, tab] = a.split(':'); d.open(id, { tab }); }); if (P.has('launcher')) d.openLauncher(); if (P.has('notify')) d.toggleNotify(); if (P.has('overview')) d.toggleOverview(); if (P.has('lost')) d.setState('lost'); }; d.connEl.style.display === 'none' ? run() : d.onReady = run; }
  const md = P.get('modal'); if (md) setTimeout(() => ({ first: showFirstRun, form: () => showConnForm(), edit: () => showConnForm(SERVERS[0]), import: showImport, hostkey: () => hostKeyDialog(SERVERS[3], () => {}, () => {}), hostkey1: () => hostKeyDialog(SERVERS[4], () => {}, () => {}, true), danger: () => confirmDanger({ title: 'Перезагрузка сервера', server: 'web-01', prod: true, typeName: 'web-01', okText: 'Перезагрузить', lines: ['Все SSH-сессии будут прерваны.'] }) }[md] || (() => {}))(), 200);
  else if (!localStorage.getItem('sherpa.first') && !P.has('fast') && !dk) setTimeout(showFirstRun, 300);
  if (P.has('selftest')) setTimeout(selftest, 400);
}
async function selftest() {
  const out = []; const T = (n, f) => { try { f(); out.push('ok  ' + n); } catch (e) { out.push('ERR ' + n + ': ' + e.message); window.__errs.push(n + ': ' + e.stack); } };
  const sleep = ms => new Promise(r => setTimeout(r, ms)); const click = el => el && el.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  const srv = SERVERS[0]; const d = openDesk(srv); await sleep(1800);
  for (const id of Object.keys(APPS)) { if (id === 'editor') continue; T('open ' + id, () => { const w = d.open(id, { fresh: true }); if (!w || !w.body.children.length) throw new Error('пустое окно'); }); }
  await sleep(1500);
  T('terminal cmds', () => { const w = d.wins.find(w => w.app.id === 'terminal'); ['help', 'ls -la /etc', 'cat /etc/hostname', 'df -h', 'systemctl status nginx', 'docker ps', 'uptime', 'free -h', 'cd /var/log', 'pwd', 'neofetch', 'foo'].forEach(c => w.api.run(c)); const t = w.body.textContent; if (!t.includes('/var/log') || !t.includes('команда не найдена')) throw new Error('вывод терминала'); });
  T('editor', () => { const w = d.open('editor', { file: '/etc/hostname', fresh: true }); if (!w.body.querySelector('textarea')) throw new Error('нет textarea'); });
  for (const [id, tabs] of [['net', ['rt', 'dns', 'ports', 'fw', 'diag', 'if']], ['system', ['time', 'usr', 'pkg', 'svc', 'gen']], ['disks', ['map', 'big', 'clean', 'parts']], ['monitor', ['pr', 'nt', 'ov']], ['notify', ['r', 'a', 'n']]]) {
    const w = d.wins.find(w => w.app.id === id); for (const t of tabs) { T(id + ':' + t, () => w.api.tab(t, {})); await sleep(150); } }
  T('launcher', () => { d.openLauncher(); d.search.value = 'nginx'; d.renderLauncher(); if (!d.gridEl.children.length) throw new Error('поиск пуст'); d.closeLauncher(); });
  T('wm', () => { const w = d.wins[0]; w.minimize(); w.restore(); w.toggleMax(); w.toggleMax(); w.close(); });
  T('modals', () => { showConnForm(); showImport(); hostKeyDialog(SERVERS[3], () => {}, () => {}); showFirstRun(); confirmDanger({ title: 't', server: 'x', typeName: 'x', lines: [] }); $$('.scrim').forEach(x => x.remove()); });
  T('drag+snap+resize', () => { const w = d.open('files', { fresh: true }); const pe = (t, el, x, y) => (el || document).dispatchEvent(new PointerEvent(t, { bubbles: true, clientX: x, clientY: y, button: 0 }));
    const r = w.bar.getBoundingClientRect(); pe('pointerdown', w.bar, r.left + 100, r.top + 10); pe('pointermove', document, r.left + 160, r.top + 60); const moved = w.el.offsetLeft; pe('pointermove', document, 2, 300); pe('pointerup', document, 2, 300);
    if (!w.max || w.el.style.left !== '0px') throw new Error('snap left не сработал: ' + w.el.style.left + ' max=' + w.max); w.toggleMax(); w.toggleMax();
    const w2 = d.open('docker', { fresh: true }); w2.el.classList.remove('anim'); const w0 = w2.el.offsetWidth; const rs = w2.el.querySelector('.rs.se'), rr = rs.getBoundingClientRect(); pe('pointerdown', rs, rr.left + 3, rr.top + 3); pe('pointermove', document, rr.left + 103, rr.top + 53); pe('pointerup', document, 0, 0); if (w2.el.offsetWidth !== w0 + 100) throw new Error('resize: ' + w0 + '→' + w2.el.offsetWidth); });
  T('overview', () => { d.toggleOverview(); d.toggleOverview(); d.setState('lost'); d.setState('ready'); });
  T('desk2', () => { openDesk(SERVERS[5]); });
  await sleep(1200);
  const el = h('pre#selftest', { style: 'position:fixed;left:0;top:0;z-index:99999;background:#000;color:#0f0;padding:8px;font:11px monospace;max-height:100vh;overflow:auto' }, out.join('\n') + '\nERRORS: ' + window.__errs.length + '\n' + window.__errs.join('\n'));
  document.body.append(el); document.title = 'SELFTEST ' + (window.__errs.length || out.some(x => x.startsWith('ERR')) ? 'FAIL' : 'OK');
}
boot();
