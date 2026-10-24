//! Unified diff с одним полным контекстным блоком; сохраняет отсутствие LF.
pub fn unified(left: &str, right: &str) -> String {
    if left == right {
        return String::new();
    }
    let a: Vec<_> = left.split_inclusive('\n').collect();
    let b: Vec<_> = right.split_inclusive('\n').collect();
    let mut out = format!(
        "--- left\n+++ right\n@@ -{},{} +{},{} @@\n",
        usize::from(!a.is_empty()),
        a.len(),
        usize::from(!b.is_empty()),
        b.len()
    );
    // Общие начало и конец показываем контекстом, середину заменяем.
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    for line in &a[..prefix] {
        append(&mut out, ' ', line);
    }
    for line in &a[prefix..a.len() - suffix] {
        append(&mut out, '-', line);
    }
    for line in &b[prefix..b.len() - suffix] {
        append(&mut out, '+', line);
    }
    for line in &a[a.len() - suffix..] {
        append(&mut out, ' ', line);
    }
    out
}
fn append(out: &mut String, prefix: char, line: &str) {
    out.push(prefix);
    out.push_str(line);
    if !line.ends_with('\n') {
        out.push_str("\n\\ No newline at end of file\n");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changes() {
        assert_eq!(
            unified("a\nb\n", "a\nc\n"),
            "--- left\n+++ right\n@@ -1,2 +1,2 @@\n a\n-b\n+c\n"
        );
        assert!(unified("x", "x").is_empty());
        assert!(unified("", "x").contains("-0,0 +1,1"));
        assert!(unified("x", "y").contains("No newline"));
    }
}
