use std::path::PathBuf;

/// Linux file managers often place `file://` URIs in `text/plain` (and sometimes
/// `x-special/gnome-copied-files` content) without a usable `text/uri-list` entry.
pub(crate) fn parse_file_paths_from_clipboard_text(text: &str) -> Vec<PathBuf> {
    use percent_encoding::percent_decode;

    let mut lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();

    if lines
        .first()
        .is_some_and(|line| *line == "copy" || *line == "cut")
    {
        lines.remove(0);
    }

    if lines.is_empty() {
        return Vec::new();
    }

    let mut paths = Vec::with_capacity(lines.len());
    for line in lines {
        let path = if let Some(uri) = line.strip_prefix("file://") {
            percent_decode(uri.as_bytes())
                .decode_utf8()
                .ok()
                .map(|decoded| PathBuf::from(decoded.as_ref()))
        } else if line.starts_with('/') {
            Some(PathBuf::from(line))
        } else {
            None
        };

        let Some(path) = path else {
            return Vec::new();
        };
        paths.push(path);
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_file_uri() {
        let paths = parse_file_paths_from_clipboard_text("file:///home/user/doc.txt");
        assert_eq!(paths, vec![PathBuf::from("/home/user/doc.txt")]);
    }

    #[test]
    fn parses_gnome_copied_files_format() {
        let text = "copy\nfile:///home/user/a.txt\nfile:///home/user/b.txt";
        let paths = parse_file_paths_from_clipboard_text(text);
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/home/user/a.txt"),
                PathBuf::from("/home/user/b.txt"),
            ]
        );
    }

    #[test]
    fn rejects_mixed_file_and_plain_text() {
        let text = "file:///home/user/a.txt\nhello";
        assert!(parse_file_paths_from_clipboard_text(text).is_empty());
    }
}
