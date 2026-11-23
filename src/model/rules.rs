//! The routing decision: which target a link opens in.
//!
//! Rules are evaluated top to bottom and the first enabled match wins. A rule
//! matches when any of its URL conditions (domain / wildcard / regex) match —
//! or it has none — and, if it has source-app conditions, any of those match
//! too. So "github.com opened from Slack" is a single rule.

use globset::{GlobBuilder, GlobMatcher};
use regex::Regex;

use super::config::{Config, Fallback, Matcher, Rule, Target};

/// Everything known about one link being opened.
#[derive(Clone, Debug, Default)]
pub struct LinkContext {
    pub url: String,
    /// File name of the app that opened the link, e.g. `slack.exe`.
    pub source_exe: Option<String>,
    /// The user held Shift: always ask.
    pub force_picker: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Open in `target`; `rule` is the id of the matching rule (`None` for the default).
    Open { target: Target, rule: Option<String> },
    /// Show the picker.
    Ask,
}

enum UrlMatcher {
    Domain(String),
    Glob(GlobMatcher),
    Regex(Regex),
}

/// A rule with its patterns compiled.
pub struct CompiledRule<'a> {
    pub rule: &'a Rule,
    url: Vec<UrlMatcher>,
    sources: Vec<String>,
    /// First invalid pattern, if any. Such a rule never matches.
    pub error: Option<String>,
}

/// Normalizes what a user typed as a domain: lowercase, no scheme, path,
/// leading `*.` / `.` or trailing dot.
pub fn normalize_domain(input: &str) -> String {
    let mut s = input.trim().to_ascii_lowercase();
    if let Some((_, rest)) = s.split_once("://") {
        s = rest.to_string();
    }
    let s = s.split(['/', '?', '#']).next().unwrap_or("");
    let s = s.rsplit_once('@').map_or(s, |(_, host)| host);
    let s = s.split(':').next().unwrap_or("");
    s.trim_start_matches("*.").trim_start_matches('.').trim_end_matches('.').to_string()
}

/// Lowercased host of a URL, if it has one.
pub fn host_of(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    let host = parsed.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    Some(host.trim_start_matches('[').trim_end_matches(']').to_string())
}

fn domain_matches(host: &str, domain: &str) -> bool {
    !domain.is_empty()
        && (host == domain || host.strip_suffix(domain).is_some_and(|rest| rest.ends_with('.')))
}

/// Checks a single pattern; `Err` carries a user-facing message.
pub fn validate(m: &Matcher) -> Result<(), String> {
    match m {
        Matcher::Domain(d) if normalize_domain(d).is_empty() => Err("Enter a domain".into()),
        Matcher::Wildcard(p) if p.trim().is_empty() => Err("Enter a pattern".into()),
        Matcher::Regex(p) if p.trim().is_empty() => Err("Enter a regular expression".into()),
        Matcher::SourceApp(a) if a.trim().is_empty() => Err("Enter an app, e.g. slack.exe".into()),
        Matcher::Wildcard(p) => compile_glob(p).map(|_| ()),
        Matcher::Regex(p) => Regex::new(p.trim()).map(|_| ()).map_err(|e| {
            // Syntax errors are multi-line with a caret diagram; keep the last line.
            let text = e.to_string();
            let reason = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
            format!("Invalid regex: {}", reason.trim_start_matches("error: "))
        }),
        _ => Ok(()),
    }
}

fn compile_glob(pattern: &str) -> Result<GlobMatcher, String> {
    GlobBuilder::new(pattern.trim())
        .case_insensitive(true)
        .literal_separator(false)
        .backslash_escape(true)
        .build()
        .map(|g| g.compile_matcher())
        .map_err(|e| format!("Invalid wildcard: {e}"))
}

/// Normalizes a source-app pattern: lowercase file name, `.exe` implied.
pub fn normalize_app(input: &str) -> String {
    let name = input.trim().rsplit(['\\', '/']).next().unwrap_or("").to_ascii_lowercase();
    if name.is_empty() || name.ends_with(".exe") { name } else { format!("{name}.exe") }
}

impl<'a> CompiledRule<'a> {
    pub fn new(rule: &'a Rule) -> Self {
        let mut url = Vec::new();
        let mut sources = Vec::new();
        let mut error = None;
        for m in &rule.matchers {
            if let Err(e) = validate(m) {
                error.get_or_insert(e);
                continue;
            }
            match m {
                Matcher::Domain(d) => url.push(UrlMatcher::Domain(normalize_domain(d))),
                Matcher::Wildcard(p) => url.push(UrlMatcher::Glob(compile_glob(p).unwrap())),
                Matcher::Regex(p) => url.push(UrlMatcher::Regex(Regex::new(p.trim()).unwrap())),
                Matcher::SourceApp(a) => sources.push(normalize_app(a)),
            }
        }
        if rule.matchers.is_empty() {
            error = Some("Add at least one condition".into());
        }
        Self { rule, url, sources, error }
    }

    pub fn matches(&self, ctx: &LinkContext) -> bool {
        if !self.rule.enabled || self.error.is_some() {
            return false;
        }
        let url = ctx.url.trim();
        let host = host_of(url);
        let url_ok = self.url.is_empty()
            || self.url.iter().any(|m| match m {
                UrlMatcher::Domain(d) => host.as_deref().is_some_and(|h| domain_matches(h, d)),
                UrlMatcher::Glob(g) => g.is_match(url),
                UrlMatcher::Regex(r) => r.is_match(url),
            });
        let source_ok = self.sources.is_empty()
            || ctx
                .source_exe
                .as_deref()
                .map(normalize_app)
                .is_some_and(|s| self.sources.contains(&s));
        url_ok && source_ok
    }
}

/// Decides where `ctx` opens. `available` says whether a target's browser is
/// installed; rules pointing at missing browsers are skipped.
pub fn route(config: &Config, ctx: &LinkContext, available: impl Fn(&Target) -> bool) -> Decision {
    if ctx.force_picker {
        return Decision::Ask;
    }
    if let Some(rule) = config
        .rules
        .iter()
        .map(CompiledRule::new)
        .find(|r| r.matches(ctx) && available(&r.rule.target))
    {
        return Decision::Open { target: rule.rule.target.clone(), rule: Some(rule.rule.id.clone()) };
    }
    match (&config.fallback, &config.default_target) {
        (Fallback::Default, Some(t)) if available(t) => Decision::Open { target: t.clone(), rule: None },
        _ => Decision::Ask,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(b: &str) -> Target {
        Target { browser: b.into(), ..Default::default() }
    }

    fn rule(id: &str, matchers: Vec<Matcher>, browser: &str) -> Rule {
        Rule { id: id.into(), name: id.into(), enabled: true, matchers, target: target(browser) }
    }

    fn ctx(url: &str, source: Option<&str>) -> LinkContext {
        LinkContext { url: url.into(), source_exe: source.map(Into::into), force_picker: false }
    }

    fn matches(m: Matcher, url: &str) -> bool {
        let r = rule("r", vec![m], "b");
        CompiledRule::new(&r).matches(&ctx(url, None))
    }

    #[test]
    fn domain_matches_host_and_subdomains_only() {
        let d = || Matcher::Domain("GitHub.com".into());
        assert!(matches(d(), "https://github.com/x"));
        assert!(matches(d(), "https://gist.github.com/x"));
        assert!(matches(d(), "http://GITHUB.COM.:8080/"));
        assert!(!matches(d(), "https://notgithub.com/"));
        assert!(!matches(d(), "https://github.com.evil.io/"));
        assert!(!matches(d(), "https://evil.io/?u=github.com"));
        assert!(matches(Matcher::Domain("*.atlassian.net".into()), "https://acme.atlassian.net/browse/X-1"));
        assert!(matches(Matcher::Domain("https://github.com/foo".into()), "https://github.com/"));
    }

    #[test]
    fn normalizes_domains_and_apps() {
        assert_eq!(normalize_domain(" https://User@Example.COM:443/path?q "), "example.com");
        assert_eq!(normalize_domain(".example.com."), "example.com");
        assert_eq!(normalize_app("Slack"), "slack.exe");
        assert_eq!(normalize_app(r"C:\Program Files\Slack\SLACK.EXE"), "slack.exe");
    }

    #[test]
    fn wildcard_and_regex() {
        let w = || Matcher::Wildcard("*://*.atlassian.net/*".into());
        assert!(matches(w(), "https://acme.atlassian.net/wiki/x"));
        assert!(!matches(w(), "https://atlassian.com/"));
        assert!(matches(Matcher::Wildcard("https://YOUTUBE.com/*".into()), "https://youtube.com/watch?v=1"));
        let r = || Matcher::Regex(r"^https://(www\.)?youtube\.com/".into());
        assert!(matches(r(), "https://www.youtube.com/watch"));
        assert!(!matches(r(), "https://m.youtube.com/watch"));
    }

    #[test]
    fn invalid_patterns_never_match() {
        let r = rule("r", vec![Matcher::Regex("(".into()), Matcher::Domain("a.com".into())], "b");
        let c = CompiledRule::new(&r);
        assert_eq!(c.error.as_deref(), Some("Invalid regex: unclosed group"));
        assert!(!c.matches(&ctx("https://a.com", None)));
        let empty = rule("e", vec![], "b");
        assert!(!CompiledRule::new(&empty).matches(&ctx("https://a.com", None)));
    }

    #[test]
    fn source_app_is_and_combined_with_url() {
        let r = rule("r", vec![Matcher::Domain("github.com".into()), Matcher::SourceApp("Slack".into())], "b");
        let c = CompiledRule::new(&r);
        assert!(c.matches(&ctx("https://github.com", Some("slack.exe"))));
        assert!(!c.matches(&ctx("https://github.com", Some("teams.exe"))));
        assert!(!c.matches(&ctx("https://github.com", None)));
        assert!(!c.matches(&ctx("https://gitlab.com", Some("slack.exe"))));
        let only_source = rule("s", vec![Matcher::SourceApp("outlook.exe".into())], "b");
        assert!(CompiledRule::new(&only_source).matches(&ctx("https://any.thing", Some("OUTLOOK.EXE"))));
    }

    #[test]
    fn routing_order_fallback_and_availability() {
        let mut config = Config {
            rules: vec![
                rule("disabled", vec![Matcher::Domain("a.com".into())], "x"),
                rule("missing", vec![Matcher::Domain("a.com".into())], "gone"),
                rule("first", vec![Matcher::Domain("a.com".into())], "chrome"),
                rule("second", vec![Matcher::Domain("a.com".into())], "edge"),
            ],
            default_target: Some(target("firefox")),
            ..Default::default()
        };
        config.rules[0].enabled = false;
        let available = |t: &Target| t.browser != "gone";

        let d = route(&config, &ctx("https://a.com", None), available);
        assert_eq!(d, Decision::Open { target: target("chrome"), rule: Some("first".into()) });

        let d = route(&config, &ctx("https://b.com", None), available);
        assert_eq!(d, Decision::Open { target: target("firefox"), rule: None });

        let forced = LinkContext { force_picker: true, ..ctx("https://a.com", None) };
        assert_eq!(route(&config, &forced, available), Decision::Ask);

        config.fallback = Fallback::Picker;
        assert_eq!(route(&config, &ctx("https://b.com", None), available), Decision::Ask);

        config.fallback = Fallback::Default;
        config.default_target = Some(target("gone"));
        assert_eq!(route(&config, &ctx("https://b.com", None), available), Decision::Ask);
    }
}
