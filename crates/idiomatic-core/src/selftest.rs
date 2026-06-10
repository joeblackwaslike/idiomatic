//! Each idiom is self-testing: its `examples.bad` must trip, `examples.good`
//! must pass, and for `autofix` idioms `autofix(bad)` must equal `good`.
use crate::engine::{autofix_source, lint_source, support_lang, CompiledIdiom};
use crate::pack::FixPolicy;
use crate::resolve::{Idiom, IdiomSet};

#[derive(Debug)]
pub struct SelfTestResult {
    pub id: String,
    pub passed: bool,
    pub detail: String,
}

fn pass(id: &str) -> SelfTestResult {
    SelfTestResult { id: id.into(), passed: true, detail: "ok".into() }
}
fn fail(id: &str, detail: impl Into<String>) -> SelfTestResult {
    SelfTestResult { id: id.into(), passed: false, detail: detail.into() }
}

fn test_one(idiom: &Idiom) -> SelfTestResult {
    // skill-only idioms have no detector — nothing to self-test.
    if idiom.fix_policy == FixPolicy::SkillOnly {
        return pass(&idiom.id);
    }
    let Some(examples) = &idiom.examples else { return pass(&idiom.id) };
    let lang = match support_lang(&idiom.language) {
        Some(l) => l,
        None => return fail(&idiom.id, "unknown language"),
    };
    let compiled = match CompiledIdiom::compile(idiom) {
        Ok(c) => vec![c],
        Err(e) => return fail(&idiom.id, format!("compile error: {e}")),
    };

    if let Some(bad) = &examples.bad {
        if lint_source(&compiled, lang, bad).is_empty() {
            return fail(&idiom.id, "examples.bad did not trip the rule");
        }
    }
    if let Some(good) = &examples.good {
        if !lint_source(&compiled, lang, good).is_empty() {
            return fail(&idiom.id, "examples.good incorrectly tripped the rule");
        }
    }
    if idiom.fix_policy == FixPolicy::Autofix {
        if let (Some(bad), Some(good)) = (&examples.bad, &examples.good) {
            let (fixed, _) = autofix_source(&compiled, lang, bad);
            if &fixed != good {
                return fail(&idiom.id, format!("autofix(bad) != good: got {fixed:?}"));
            }
        }
    }
    pass(&idiom.id)
}

/// Run self-tests for every idiom in the set.
pub fn run_selftests(set: &IdiomSet) -> Vec<SelfTestResult> {
    set.iter().map(test_one).collect()
}
