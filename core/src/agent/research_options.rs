//! Shared research policy. Hosts own transport, browser binding and persistence.
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::agent::{Backend, Tool};
use crate::cdp::PageSession;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResearchOptions {
    pub deep: bool,
    pub workflow: String,
    pub custom_instructions: String,
    pub automatic_sources: bool,
    /// Host-enabled implicit web worker; not a source-picker selection.
    pub web_search: bool,
}
impl ResearchOptions {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(matches!(self.workflow.as_str(), "" | "general" | "vc" | "custom"), "Unknown expert mode");
        anyhow::ensure!(self.custom_instructions.len() <= 160000, "Expert instructions too long");
        anyhow::ensure!(self.workflow != "custom" || !self.custom_instructions.trim().is_empty(), "Custom expert instructions missing");
        Ok(())
    }
    pub fn workflow_instructions(&self) -> String {
        match self.workflow.as_str() {
            "vc" => include_str!("skills/vc-research/SKILL.md").into(),
            "custom" => self.custom_instructions.clone(),
            _ => String::new(),
        }
    }
    pub fn initial_skills(&self) -> Vec<String> {
        let mut skills = Vec::new();
        if self.workflow == "vc" { skills.push("vc-research".into()); }
        if self.deep { skills.push("research".into()); }
        skills
    }
    pub fn max_steps(&self) -> u32 {
        if self.deep { 120 } else { super::r#loop::DEFAULT_MAX_STEPS }
    }
    pub fn instructions(&self) -> String {
        let depth = if self.deep {
            "Research depth: deep. Explore complementary directions before converging; follow promising signals selectively. Use the larger budget for useful discovery, not repeated verification or a quota. Reserve time for delivery."
        } else {
            "Research depth: quick. Produce a useful first pass within the smaller budget, preserving relevant uncertain leads. Do not turn this into exhaustive research or diligence. Loading a skill does not change this budget."
        };
        let depth = if self.deep { format!("{depth}\n\n{COMMENT_FEEDBACK_GUIDANCE}") } else { depth.to_string() };
        let depth = if self.deep && self.web_search { format!("{depth}\n\n{WEB_SEARCH_GUIDANCE}") } else { depth };
        format!("{}\n\n{}\n\n{}\n\n{}\n\n{}", depth, self.workflow_instructions(), WORKBENCH_GUIDANCE, super::research_board::RELATION_GUIDANCE,
            if self.automatic_sources { "Choose one or more available platforms based on the user's question. For broad VC discovery default to xhs, x, weixin, podcast and github when available. Explain missing coverage or access failures." } else { "Only collect new social-platform evidence from selected platforms. Use supplementary public-web research only when explicitly enabled by the deep-mode guidance above. Explain missing coverage or access failures; never bypass explicit user source restrictions." })
    }
    pub fn tools(&self, backend: Arc<dyn Backend>, page: Option<Arc<PageSession>>, mut sites: Vec<String>, language: Option<String>, hosted_policy: String) -> Vec<Arc<dyn Tool>> {
        if self.deep && self.web_search && !sites.iter().any(|site| site == "web") { sites.push("web".into()); }
        let board = Arc::new(super::research_board::ResearchBoardTool::default());
        vec![board.clone(), Arc::new(crate::sites::explore::ExplorePlatformsTool {
            backend, page, sites, language, workflow: self.workflow_instructions(), board, deep: self.deep, hosted_policy,
        })]
    }
}
pub const WORKBENCH_GUIDANCE: &str = "Research workbench: use update_research_board early to state platform focus and a short plan; add useful leads incrementally as found, including incomplete signals, and update next_steps when direction changes. This surface shows concise findings and actions, never private reasoning. Update after each useful discovery batch, without waiting for final synthesis. Include every distinct useful team/product/direction as a node even when no relationship is known. After each platform round, reconcile duplicate entities across platforms and add concrete sourced founder, team, product, dependency or collaboration relations. Never invent an edge merely to fill the graph. Keep sourced relationships distinct from hypotheses. For deep research, also save and publish a CSV or XLSX findings table with names, platforms, signals, source URLs, uncertainties and next steps; produce it once at delivery, even if only a few useful leads are available. Never invent rows to fill it.";

pub const COMMENT_FEEDBACK_GUIDANCE: &str = "Incidental comment feedback in deep research: when reading each post on a platform with comments or replies (such as XHS, X or Douyin), also skim the first few available comments, typically 3–5, using the platform's actual read tools. Reuse comments already returned or saved; if a separate read is needed, request only a small first batch. Do not routinely paginate, expand reply chains or launch separate sentiment searches. Respect explicit user limits and report unavailable comments without blocking the main research. Retain relevant praise, complaints, disagreements, unmet needs and repeated themes as supplementary evidence, clearly distinguishing commenter views from the post author's claims. Cite the original post and an observed comment/reply link when available. Treat this small, platform-ordered sample as anecdotal: do not infer population sentiment, percentages or a trend from one isolated opinion. Include useful feedback in the synthesis or a brief comment-feedback section when warranted, without forcing a section when no useful signal was found. Pass this lightweight sampling scope to delegated branches and retain their comment findings when synthesizing. If the user explicitly requests reputation, sentiment or comment analysis, follow their requested scope and depth instead of this incidental sampling default.";

pub const WEB_SEARCH_GUIDANCE: &str = "Deep research includes supplementary public-web research by default, even when the source picker lists only social platforms or HTTP-only sources. Start explore_platforms early with a web branch alongside the useful selected-platform branches in the same call, including for single-platform research. The web worker owns a separate browser tab and uses DuckDuckGo, then reads useful original pages. Pass the user's full goal, explicit source restrictions and known context. This is an implicit deep-mode supplement, not an extra source-picker platform; selected-platform restrictions still apply to social platforms. Explicit user instructions excluding web or limiting research to particular sources override this default. Integrate source-linked web findings with platform evidence, label snippet-only leads, and disclose web access failures without blocking useful platform research. Follow up selectively instead of repeating web searches in every round.";

use std::{path::Path, io::Write};
use serde_json::{json, Value};
/// Deep research always leaves a tabular handoff, including early signals.
/// Prefer a table the agent already published; otherwise export the workbench.
pub fn ensure_research_table(run_dir: &Path) -> anyhow::Result<()> {
    let outputs = run_dir.join("outputs");
    if outputs.is_symlink() { anyhow::bail!("outputs must not be a symlink"); }
    if std::fs::read_dir(&outputs).ok().into_iter().flatten().filter_map(Result::ok).any(|e| {
        e.path().extension().and_then(|s| s.to_str()).is_some_and(|s| matches!(s.to_ascii_lowercase().as_str(), "csv" | "xlsx"))
    }) { return Ok(()); }
    let board: Value = std::fs::read(run_dir.join("research-board.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    let cell = |value: &str| {
        let safe = if value.trim_start().starts_with(['=', '+', '-', '@']) { format!("'{value}") } else { value.to_string() };
        format!("\"{}\"", safe.replace('"', "\"\""))
    };
    let mut csv = String::from("\u{feff}名称,平台,信号或内容,分类,未知与限制,来源链接,下一步\r\n");
    let next = board["next_steps"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("；")).unwrap_or_default();
    if let Some(leads) = board["leads"].as_array().filter(|a| !a.is_empty()) {
        for row in leads {
            let values = [row["name"].as_str().unwrap_or(""), row["platform"].as_str().unwrap_or(""), row["signal"].as_str().unwrap_or(""), row["stage"].as_str().unwrap_or(""), row["uncertainty"].as_str().unwrap_or(""), row["url"].as_str().unwrap_or(""), &next];
            csv.push_str(&values.into_iter().map(cell).collect::<Vec<_>>().join(",")); csv.push_str("\r\n");
        }
    } else {
        for row in crate::agent::note_store::load_notes(run_dir) {
            let values = [row["title"].as_str().unwrap_or(""), row["site"].as_str().unwrap_or(""), row["excerpt"].as_str().unwrap_or(""), "来源记录", "尚未整理为候选线索；请结合研究报告阅读", row["url"].as_str().unwrap_or(""), ""];
            csv.push_str(&values.into_iter().map(cell).collect::<Vec<_>>().join(",")); csv.push_str("\r\n");
        }
    }
    std::fs::create_dir_all(&outputs)?;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(outputs.join("research-findings.csv"))?;
    file.write_all(csv.as_bytes())?;
    Ok(())
}

/// Observed archive statistics, identical on every host.
pub fn reading_stats(dir: &Path, index: usize) -> Vec<Value> {
    let mut result = Vec::new();
            if let Some(stats) = crate::sites::xhs::reading_stats::for_run(&dir) {
                result.push(json!({"turn_index":index,"site":"xhs","authors_kind":"profiles","preview_notes":stats.preview_notes,"detail_notes":stats.detail_notes,"comments":stats.comments,"authors":stats.authors}));
            }
            let mut sites: std::collections::BTreeMap<String, (usize,usize,usize,std::collections::BTreeSet<String>)> = Default::default();
            fn count_comments(value: &Value) -> usize {
                value.as_array().map(|items| items.iter().map(|c| usize::from(c["text"].as_str().is_some_and(|s| !s.is_empty())) + count_comments(&c["replies"])).sum()).unwrap_or(0)
            }
            for note in crate::agent::note_store::load_notes(&dir) {
                let site = note["site"].as_str().unwrap_or("xhs");
                if site == "xhs" { continue; }
                let row = sites.entry(site.to_string()).or_default();
                if note["level"] == "deep" { row.1 += 1; } else { row.0 += 1; }
                row.2 += count_comments(&note["comments"]);
                if let Some(author) = note["author"]["url"].as_str().or_else(|| note["author"]["handle"].as_str()).or_else(|| note["author"]["name"].as_str()).filter(|s| !s.is_empty()) { row.3.insert(author.to_string()); }
            }
            for (site, (preview,detail,comments,authors)) in sites {
                result.push(json!({"turn_index":index,"site":site,"authors_kind":"observed","preview_notes":preview,"detail_notes":detail,"comments":comments,"authors":authors.len()}));
            }
    result
}
