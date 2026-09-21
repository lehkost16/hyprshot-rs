use std::process::Command;

pub fn send(summary: &str, body: impl AsRef<str>, icon: &str) {
    let _ = Command::new("notify-send")
        .arg(summary)
        .arg(body.as_ref())
        .arg("-i")
        .arg(icon)
        .spawn();
}

pub fn saved(path: impl std::fmt::Display) {
    send(
        "Annotator",
        format!("已成功保存图片至 {}", path),
        "document-save",
    );
}

pub fn copied() {
    send("Annotator", "已成功复制到剪贴板", "edit-copy");
}
