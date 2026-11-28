//! Shared file type presentation for the composer and transcript.

use gpui::Hsla;

use crate::theme::Palette;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileFormat {
    Pdf,
    Spreadsheet,
    Document,
    Presentation,
    Archive,
    Code,
    Generic,
}

impl FileFormat {
    pub(super) fn detect(name: &str, mime: &str) -> Self {
        let mime = mime.split(';').next().unwrap_or_default().trim();
        match mime {
            "application/pdf" => return Self::Pdf,
            "application/vnd.ms-excel"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.oasis.opendocument.spreadsheet"
            | "text/csv" => return Self::Spreadsheet,
            "application/msword"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.oasis.opendocument.text"
            | "application/rtf" => return Self::Document,
            "application/vnd.ms-powerpoint"
            | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            | "application/vnd.oasis.opendocument.presentation" => return Self::Presentation,
            "application/zip"
            | "application/x-7z-compressed"
            | "application/vnd.rar"
            | "application/gzip"
            | "application/x-tar" => return Self::Archive,
            _ => {}
        }
        match name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .as_deref()
        {
            Some("pdf") => Self::Pdf,
            Some("xlsx" | "xls" | "xlsm" | "xlsb" | "csv" | "tsv" | "ods" | "numbers") => {
                Self::Spreadsheet
            }
            Some("doc" | "docx" | "docm" | "rtf" | "odt" | "pages") => Self::Document,
            Some("ppt" | "pptx" | "pptm" | "odp" | "key") => Self::Presentation,
            Some("zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar") => Self::Archive,
            Some(
                "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "json" | "xml" | "html" | "css"
                | "toml" | "yaml" | "yml" | "sh" | "sql",
            ) => Self::Code,
            _ => Self::Generic,
        }
    }

    pub(super) fn label(self, name: &str) -> &'static str {
        match self {
            Self::Pdf => "PDF 文档",
            Self::Spreadsheet if has_suffix(name, &["csv", "tsv", "ods", "numbers"]) => "电子表格",
            Self::Spreadsheet => "Excel 工作簿",
            Self::Document if has_suffix(name, &["rtf", "odt", "pages"]) => "文档",
            Self::Document => "Word 文档",
            Self::Presentation if has_suffix(name, &["odp", "key"]) => "演示文稿",
            Self::Presentation => "PowerPoint 演示文稿",
            Self::Archive => "压缩包",
            Self::Code => "代码文件",
            Self::Generic => "文件",
        }
    }

    pub(super) fn icon(self) -> &'static str {
        match self {
            Self::Pdf => "file-pdf",
            Self::Spreadsheet => "file-spreadsheet",
            Self::Document | Self::Generic => "file-text",
            Self::Presentation => "presentation",
            Self::Archive => "archive-box",
            Self::Code => "code-xml",
        }
    }

    pub(super) fn accent(self, palette: Palette) -> Option<Hsla> {
        let dark = palette.bg.l < 0.45;
        let (light, dark_color) = match self {
            Self::Pdf => (0xaa322f, 0xffa6a0),
            Self::Spreadsheet => (0x176143, 0x8ed6ae),
            Self::Document => (0x235994, 0x98c7ff),
            Self::Presentation => (0x9c4620, 0xffb18e),
            Self::Archive => (0x6b4990, 0xd4b7f7),
            Self::Code => (0x3a5a6b, 0x98c4d6),
            Self::Generic => return None,
        };
        Some(gpui::rgb(if dark { dark_color } else { light }).into())
    }
}

pub(super) fn has_suffix(name: &str, suffixes: &[&str]) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, suffix)| suffixes.iter().any(|ext| suffix.eq_ignore_ascii_case(ext)))
}
