//! SigmaOS-apt-extracttemplates — debconf Templates file parser.
//!
//! Inspired by Debian's `apt-extracttemplates` / `debconf`: parses the
//! `Templates` index (which ships package template text for `debconf`) into
//! structured records — `Template:` name, `Description:` short+long text,
//! optional `Description-md5`, and `Type`/`Section` fields — so templates can
//! be looked up by package or name.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;

/// Errors raised by this component.
#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    /// A stanza was missing its `Template` field.
    #[error("stanza missing Template field")]
    MissingTemplate,
    /// A continuation line appeared with no active field.
    #[error("orphan continuation line")]
    OrphanContinuation,
}

/// Result alias for component operations.
pub type ComponentResult<T> = Result<T, ComponentError>;

/// One parsed template record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Template {
    /// Template name (`package/question` or `package/template`).
    pub name: String,
    /// Owning package name (part before `/`).
    pub package: String,
    /// Short description (first line after `Description:`).
    pub short: String,
    /// Long description (continuation lines, joined).
    pub long: String,
    /// `Description-md5` checksum if present.
    pub md5: Option<String>,
    /// `Type` field if present.
    pub kind: Option<String>,
    /// `Section` field if present.
    pub section: Option<String>,
}

impl Template {
    /// The full description text (short + long).
    pub fn description(&self) -> String {
        if self.long.is_empty() {
            self.short.clone()
        } else {
            format!("{}\n{}", self.short, self.long)
        }
    }
}

/// Parsed Templates index.
#[derive(Debug, Default)]
pub struct Templates {
    /// template name → record.
    by_name: BTreeMap<String, Template>,
    /// package → template names it provides.
    by_package: BTreeMap<String, Vec<String>>,
}

impl Templates {
    /// Empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse `Templates` text. Stanzas are separated by blank lines;
    /// `Field: value` lines with space-indented continuations.
    pub fn parse(input: &str) -> ComponentResult<Self> {
        let mut t = Templates::new();
        let mut cur = Template::default();
        let mut have = false;
        // Track which field a continuation belongs to.
        let mut cont_field: Option<String> = None;

        for raw in input.lines() {
            let line = raw.trim_end();
            if line.trim().is_empty() {
                if have { t.insert(std::mem::take(&mut cur)); have = false; }
                cont_field = None;
                continue;
            }
            if line.starts_with(' ') || line.starts_with('\t') {
                // Continuation of the previous field's value.
                let Some(f) = &cont_field else {
                    return Err(ComponentError::OrphanContinuation);
                };
                let add = line.trim();
                match f.as_str() {
                    "Description" => {
                        if cur.long.is_empty() {
                            cur.long = add.to_string();
                        } else {
                            cur.long.push(' ');
                            cur.long.push_str(add);
                        }
                    }
                    "Description-md5" => {
                        cur.md5 = Some(format!("{} {}", cur.md5.take().unwrap_or_default(), add).trim().to_string());
                    }
                    _ => {}
                }
                continue;
            }
            cont_field = None;
            let Some((key, val)) = line.split_once(':') else { continue };
            let key = key.trim();
            let val = val.trim();
            match key {
                "Template" => {
                    if have { t.insert(std::mem::take(&mut cur)); }
                    cur.name = val.to_string();
                    cur.package = val.split('/').next().unwrap_or("").to_string();
                    have = true;
                }
                "Description" => {
                    cur.short = val.to_string();
                    cont_field = Some("Description".into());
                }
                "Description-md5" => {
                    cur.md5 = Some(val.to_string());
                    cont_field = Some("Description-md5".into());
                }
                "Type" => { cur.kind = Some(val.to_string()); }
                "Section" => { cur.section = Some(val.to_string()); }
                _ => {}
            }
        }
        if have { t.insert(cur); }
        Ok(t)
    }

    fn insert(&mut self, t: Template) {
        if t.name.is_empty() { return; }
        self.by_package.entry(t.package.clone()).or_default().push(t.name.clone());
        self.by_name.insert(t.name.clone(), t);
    }

    /// Look up a template by full name (`pkg/question`).
    pub fn get(&self, name: &str) -> Option<&Template> {
        self.by_name.get(name)
    }

    /// All templates owned by `package`.
    pub fn by_package(&self, package: &str) -> Vec<&Template> {
        self.by_package
            .get(package)
            .map(|names| names.iter().filter_map(|n| self.by_name.get(n)).collect())
            .unwrap_or_default()
    }

    /// All template names (sorted).
    pub fn names(&self) -> Vec<&str> {
        self.by_name.keys().map(String::as_str).collect()
    }

    /// Number of templates.
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TPL: &str = "Template: bash/bug\nDescription: Bash shell\n Bash is a shell. It does things.\nDescription-md5: abc123\n\nTemplate: apt/method\nDescription: APT acquisition method\n APT can acquire packages many ways.\nSection: admin\nType: string\n";

    #[test]
    fn parses_templates() {
        let t = Templates::parse(TPL).unwrap();
        assert_eq!(t.len(), 2);
        let b = t.get("bash/bug").unwrap();
        assert_eq!(b.package, "bash");
        assert_eq!(b.short, "Bash shell");
        assert!(b.long.contains("does things"));
        assert_eq!(b.md5.as_deref(), Some("abc123"));
    }

    #[test]
    fn by_package_lookup() {
        let t = Templates::parse(TPL).unwrap();
        let bash = t.by_package("bash");
        assert_eq!(bash.len(), 1);
        assert_eq!(t.by_package("apt")[0].name, "apt/method");
        assert!(t.by_package("nonexistent").is_empty());
    }

    #[test]
    fn names_sorted() {
        let t = Templates::parse(TPL).unwrap();
        assert_eq!(t.names(), vec!["apt/method", "bash/bug"]);
    }

    #[test]
    fn empty_ok() {
        assert!(Templates::parse("").unwrap().is_empty());
    }

    #[test]
    fn description_full() {
        let t = Templates::parse(TPL).unwrap();
        let d = t.get("bash/bug").unwrap().description();
        assert!(d.contains("Bash shell"));
        assert!(d.contains("does things"));
    }
}
