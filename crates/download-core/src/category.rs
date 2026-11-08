use serde::{Deserialize, Serialize};

/// A download category. Built-in categories can be extended by the user with
/// additional extensions and destination overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub extensions: Vec<String>,
    pub destination_dir: Option<String>,
    pub builtin: bool,
}

pub fn builtin_categories() -> Vec<Category> {
    let raw: [(&str, &str, &[&str]); 6] = [
        (
            "documents",
            "Documents",
            &[
                "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "md", "csv", "epub",
            ],
        ),
        (
            "images",
            "Images",
            &[
                "jpg", "jpeg", "png", "gif", "webp", "svg", "bmp", "tiff", "avif", "ico",
            ],
        ),
        (
            "videos",
            "Videos",
            &[
                "mp4", "mkv", "webm", "avi", "mov", "flv", "wmv", "m4v", "ts",
            ],
        ),
        (
            "audio",
            "Audio",
            &["mp3", "flac", "wav", "aac", "ogg", "m4a", "opus", "wma"],
        ),
        (
            "archives",
            "Archives",
            &[
                "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "zst", "iso", "dmg",
            ],
        ),
        (
            "software",
            "Software",
            &[
                "exe", "msi", "msix", "appx", "deb", "rpm", "apk", "pkg", "appimage",
            ],
        ),
    ];

    let mut categories: Vec<Category> = raw
        .iter()
        .map(|(id, name, extensions)| Category {
            id: (*id).to_string(),
            name: (*name).to_string(),
            extensions: extensions.iter().map(|s| s.to_string()).collect(),
            destination_dir: None,
            builtin: true,
        })
        .collect();
    categories.push(Category {
        id: "other".into(),
        name: "Other".into(),
        extensions: Vec::new(),
        destination_dir: None,
        builtin: true,
    });
    categories
}

/// Classify a file by extension. Unknown extensions fall into `other`.
pub fn classify_extension(file_name: &str) -> &'static str {
    let extension = std::path::Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "csv"
        | "epub" => "documents",
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" | "tiff" | "avif" | "ico" => {
            "images"
        }
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "flv" | "wmv" | "m4v" | "ts" => "videos",
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "opus" | "wma" => "audio",
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "iso" | "dmg" => "archives",
        "exe" | "msi" | "msix" | "appx" | "deb" | "rpm" | "apk" | "pkg" | "appimage" => "software",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_types_case_insensitively() {
        assert_eq!(classify_extension("Report.PDF"), "documents");
        assert_eq!(classify_extension("movie.mkv"), "videos");
        assert_eq!(classify_extension("archive.tar.gz"), "archives");
        assert_eq!(classify_extension("unknown.xyz"), "other");
        assert_eq!(classify_extension("no-extension"), "other");
    }

    #[test]
    fn builtins_are_unique() {
        let categories = builtin_categories();
        let mut ids: Vec<_> = categories.iter().map(|c| c.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), categories.len());
    }
}
