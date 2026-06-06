#[derive(Debug, Clone, Copy)]
pub struct ClipboardLimits {
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for ClipboardLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 100 * 1024 * 1024,
            max_total_bytes: 500 * 1024 * 1024,
        }
    }
}

impl ClipboardLimits {
    pub fn unlimited() -> Self {
        Self {
            max_file_bytes: u64::MAX,
            max_total_bytes: u64::MAX,
        }
    }

    pub fn validate_files(&self, files: &[(String, Vec<u8>)]) -> Result<(), String> {
        let mut total = 0u64;
        for (name, data) in files {
            let size = data.len() as u64;
            if size > self.max_file_bytes {
                return Err(format!(
                    "clipboard file `{name}` exceeds limit of {} bytes",
                    self.max_file_bytes
                ));
            }
            total = total.saturating_add(size);
        }
        if total > self.max_total_bytes {
            return Err(format!(
                "clipboard files exceed total limit of {} bytes",
                self.max_total_bytes
            ));
        }
        Ok(())
    }

    pub fn validate_entries(&self, entries: &[(String, u64)]) -> Result<(), String> {
        let mut total = 0u64;
        for (name, size) in entries {
            if *size > self.max_file_bytes {
                return Err(format!(
                    "file `{name}` exceeds limit of {} bytes",
                    self.max_file_bytes
                ));
            }
            total = total.saturating_add(*size);
        }
        if total > self.max_total_bytes {
            return Err(format!(
                "files exceed total limit of {} bytes",
                self.max_total_bytes
            ));
        }
        Ok(())
    }

    pub fn validate_blob(&self, size: usize) -> Result<(), String> {
        if size as u64 > self.max_file_bytes {
            return Err(format!(
                "clipboard blob exceeds limit of {} bytes",
                self.max_file_bytes
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_accepts_large_entries() {
        let limits = ClipboardLimits::unlimited();
        limits
            .validate_entries(&[("big.bin".to_owned(), u64::MAX)])
            .unwrap();
    }
}
