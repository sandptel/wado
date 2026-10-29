//! A line diff, for showing what `wado claude hooks install|uninstall` will change before it
//! does. Longest common subsequence over lines — settings files are small — printed with two
//! lines of context around each change, `-` and `+` the way `diff -u` reads.

/// `(mark, line)`: mark is ' ', '-' or '+'.
pub fn lines<'a>(old: &'a str, new: &'a str) -> Vec<(char, &'a str)> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let (n, m) = (a.len(), b.len());
    // lcs[i][j]: the common run of a[i..] and b[j..].
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push((' ', a[i]));
            i += 1;
            j += 1;
        } else if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
            out.push(('+', b[j]));
            j += 1;
        } else {
            out.push(('-', a[i]));
            i += 1;
        }
    }
    out
}

/// The changes with `ctx` lines of context, `…` between far-apart hunks. Empty: no change.
pub fn render(old: &str, new: &str, ctx: usize) -> Vec<String> {
    let all = lines(old, new);
    let near: Vec<bool> = (0..all.len())
        .map(|i| {
            let lo = i.saturating_sub(ctx);
            let hi = (i + ctx + 1).min(all.len());
            all[lo..hi].iter().any(|(c, _)| *c != ' ')
        })
        .collect();
    let mut out = Vec::new();
    let mut gap = false;
    for (i, (c, l)) in all.iter().enumerate() {
        if near[i] {
            if gap && !out.is_empty() {
                out.push("  …".to_string());
            }
            gap = false;
            out.push(format!("{c} {l}"));
        } else {
            gap = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_only_what_changed_with_context() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "a\nb\nc\nd\ne\nf\nX\ng\nh\n";
        assert_eq!(render(old, new, 1), vec!["  f", "+ X", "  g"]);
        assert!(render(old, old, 2).is_empty());
        let removed = render("1\n2\n3\n", "1\n3\n", 0);
        assert_eq!(removed, vec!["- 2"]);
    }
}
