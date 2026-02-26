pub fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let truncated: String = s.chars().take(max_len - 1).collect();
        format!("{truncated}...")
    } else {
        s.to_string()
    }
}

pub fn write_to_file(file_name: &str, content: &str) -> anyhow::Result<()> {
    let log_dir = std::path::Path::new("log");
    if !log_dir.exists() {
        std::fs::create_dir_all(log_dir)?;
    }
    std::fs::write(std::path::Path::new(log_dir).join(file_name), content)?;
    Ok(())
}
