use notify_rust::Notification;

pub fn send(summary: &str, body: impl AsRef<str>, icon: &str, timeout: u32) {
    let _ = Notification::new()
        .appname("Hyshot")
        .summary(summary)
        .body(body.as_ref())
        .icon(icon)
        .timeout(timeout.min(i32::MAX as u32) as i32)
        .show();
}

pub fn saved(path: impl std::fmt::Display, timeout: u32) {
    send(
        "Hyshot",
        format!("已成功保存图片至 {}", path),
        "document-save",
        timeout,
    );
}

pub fn copied(timeout: u32) {
    send("Hyshot", "已成功复制到剪贴板", "edit-copy", timeout);
}
