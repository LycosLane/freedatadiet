use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct CompiledExcludes {
    exact_names: HashSet<String>,
    prefix_names: Vec<String>,
    suffix_names: Vec<String>,
    contains_names: Vec<String>,
    path_prefixes: Vec<String>,
}

impl CompiledExcludes {
    pub fn compile(raw_patterns: &[String]) -> Self {
        let mut exact_names = HashSet::new();
        let mut prefix_names = Vec::new();
        let mut suffix_names = Vec::new();
        let mut contains_names = Vec::new();
        let mut path_prefixes = Vec::new();

        for raw in raw_patterns {
            let pat = raw.trim().to_lowercase();
            if pat.is_empty() {
                continue;
            }

            if pat.contains('/') || pat.contains('\\') {
                let norm = pat.replace('\\', "/").trim_end_matches('/').to_string();
                path_prefixes.push(norm);
            } else if pat.starts_with('*') && pat.ends_with('*') && pat.len() > 2 {
                contains_names.push(pat[1..pat.len() - 1].to_string());
            } else if let Some(suffix) = pat.strip_prefix('*') {
                suffix_names.push(suffix.to_string());
            } else if let Some(prefix) = pat.strip_suffix('*') {
                prefix_names.push(prefix.to_string());
            } else {
                exact_names.insert(pat);
            }
        }

        Self {
            exact_names,
            prefix_names,
            suffix_names,
            contains_names,
            path_prefixes,
        }
    }

    #[inline]
    pub fn matches(&self, name: &str, path: &Path) -> bool {
        if crate::model::is_dos_device_name(name) {
            return true;
        }

        if self.exact_names.iter().any(|ex| ex.eq_ignore_ascii_case(name)) {
            return true;
        }

        let lower_name = name.to_ascii_lowercase();
        if self.prefix_names.iter().any(|p| lower_name.starts_with(p))
            || self.suffix_names.iter().any(|s| lower_name.ends_with(s))
            || self.contains_names.iter().any(|c| lower_name.contains(c))
        {
            return true;
        }

        if !self.path_prefixes.is_empty() {
            let path_str = path.to_string_lossy();
            let norm_path = path_str.replace('\\', "/").to_ascii_lowercase();
            for prefix in &self.path_prefixes {
                if norm_path == *prefix || norm_path.starts_with(&format!("{}/", prefix)) {
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_matches_exclude_path_and_wildcards() {
        let excludes = CompiledExcludes::compile(&[
            "node_modules".to_string(),
            "*.tmp".to_string(),
            "temp*".to_string(),
            "*cache*".to_string(),
            "C:/Secret/Folder".to_string(),
        ]);

        assert!(excludes.matches("node_modules", &PathBuf::from("C:\\project\\node_modules")));
        assert!(excludes.matches("file.tmp", &PathBuf::from("C:\\temp\\file.tmp")));
        assert!(excludes.matches("temporary.txt", &PathBuf::from("C:\\temporary.txt")));
        assert!(excludes.matches("my_cache_dir", &PathBuf::from("C:\\my_cache_dir")));
        assert!(excludes.matches("Folder", &PathBuf::from("C:\\Secret\\Folder")));
        assert!(excludes.matches("sub", &PathBuf::from("C:\\Secret\\Folder\\sub")));
        assert!(!excludes.matches("valid.txt", &PathBuf::from("C:\\valid.txt")));
    }
}
