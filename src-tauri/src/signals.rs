//! Signal from noise: the three things that matter today, ranked with
//! reasons, learned from what the person keeps, finishes or calls noise;
//! how close each day sits to their why; the threads running through the
//! week; and the two small acts of a brain, connecting and remembering.

use crate::engine::{day_bounds, day_of};
use crate::AppState;
use std::collections::HashMap;

/// Default feature weights; the user's rankings move them (see `learn`).
pub fn default_weights() -> HashMap<String, f64> {
    [
        ("due", 3.0), ("soon", 2.5), ("waiting", 2.0), ("recurrence", 1.5), ("align", 2.0), ("stakes", 1.0),
        ("event", 1.5), ("task", 0.6), ("date", 0.8), ("person", 0.6), ("thread", 0.4), ("fresh", 0.5),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

fn weights(state: &AppState) -> HashMap<String, f64> {
    let mut w = default_weights();
    for (k, v) in &crate::lock(&state.settings).signal_weights {
        w.insert(k.clone(), *v);
    }
    w
}

/// "by friday", "on monday", "tomorrow", "today" → days until, from the text.
pub fn days_until_in(text: &str, now: i64) -> Option<i64> {
    let t = text.to_lowercase();
    if t.contains("today") || t.contains("tonight") || t.contains("this evening") {
        return Some(0);
    }
    if t.contains("tomorrow") {
        return Some(1);
    }
    let days = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
    let today_idx = {
        let offset = crate::engine::local_offset_secs();
        ((now / 1000 + offset).div_euclid(86_400) + 3).rem_euclid(7) // 1970-01-01 was a Thursday
    } as usize;
    for (i, d) in days.iter().enumerate() {
        if t.contains(d) {
            let ahead = ((i as i64) - (today_idx as i64)).rem_euclid(7);
            return Some(if ahead == 0 { 7 } else { ahead });
        }
    }
    if t.contains("this week") {
        return Some(3);
    }
    None
}

fn has_stakes(numbers: &[String], text: &str) -> bool {
    let t = text.to_lowercase();
    !numbers.is_empty() && (t.contains('₹') || t.contains('$') || t.contains('€') || t.contains("lakh") || t.contains("crore") || t.contains("usd") || t.contains("inr") || t.contains('%'))
}

struct Cand {
    kind: &'static str,
    ref_id: i64,
    activity_id: i64,
    memory_id: i64,
    title: String,
    features: HashMap<String, f64>,
    bits: Vec<String>,
}

/// Alignment of a set of memories to the person's why: memory id → 0..1.
fn alignment_map(state: &AppState) -> HashMap<i64, f64> {
    let (why, how) = {
        let s = crate::lock(&state.settings);
        (s.purpose_why.clone(), s.purpose_how.clone())
    };
    let text = format!("{}\n{}", why.trim(), how.join("\n"));
    if text.trim().chars().count() < 12 {
        return HashMap::new();
    }
    let Some(vec) = crate::engine::embed_query(state, &text) else { return HashMap::new() };
    crate::lock(&state.store).vector_search(&vec, 400).unwrap_or_default().into_iter().map(|(id, s)| (id, s.clamp(0.0, 1.0) as f64)).collect()
}

/// Build today's signals and store them. Returns how many were ranked.
pub fn refresh(state: &AppState, now: i64) -> Result<usize, String> {
    let day = day_of(now);
    let w = weights(state);
    let align = alignment_map(state);
    let mut cands: Vec<Cand> = Vec::new();
    let store = crate::lock(&state.store);

    // Open commitments: due words, who asked, stakes, freshness.
    for t in store.list_tasks("open", 300).map_err(|e| e.to_string())? {
        let mut f = HashMap::new();
        let mut bits = Vec::new();
        f.insert("task".into(), 1.0);
        if let Some(d) = days_until_in(&t.text, now) {
            f.insert("due".into(), (1.0 - d as f64 / 7.0).clamp(0.1, 1.0));
            bits.push(if d == 0 { "due today".to_string() } else if d == 1 { "due tomorrow".to_string() } else { format!("due in {d} days") });
        }
        if let Ok(Some(card)) = store.memory_by_id(t.memory_id) {
            if matches!(card.kind.as_str(), "email" | "chat" | "meeting") {
                f.insert("waiting".into(), 0.7);
                let who = card.people.iter().find(|p| !crate::engine::user_name().map_or(false, |u| p.to_lowercase().contains(&u.to_lowercase())));
                bits.push(match who { Some(p) => format!("{p} is waiting"), None => "someone is waiting".into() });
            }
            if has_stakes(&card.numbers, &format!("{} {}", card.summary, t.text)) {
                f.insert("stakes".into(), 1.0);
                if let Some(n) = card.numbers.first() { bits.push(n.clone()); }
            }
        }
        let age_days = ((now - t.created_at) / 86_400_000).max(0) as f64;
        f.insert("fresh".into(), (1.0 - age_days / 14.0).clamp(0.0, 1.0));
        cands.push(Cand { kind: "task", ref_id: t.id, activity_id: t.activity_id, memory_id: t.memory_id, title: t.text.clone(), features: f, bits });
    }
    // Dates written in memories, within a week.
    for u in store.upcoming_dates(now, 7).map_err(|e| e.to_string())? {
        let d = ((u.when - now) / 86_400_000).max(0);
        let mut f = HashMap::new();
        f.insert("date".into(), 1.0);
        f.insert("soon".into(), (1.0 - d as f64 / 7.0).clamp(0.1, 1.0));
        let title = if u.about == "mentioned" { u.title.clone() } else { format!("{} · {}", u.about, u.title) };
        let bits = vec![if d == 0 { "today".to_string() } else if d == 1 { "tomorrow".to_string() } else { format!("in {d} days") }];
        cands.push(Cand { kind: "date", ref_id: u.memory_id, activity_id: u.activity_id, memory_id: u.memory_id, title, features: f, bits });
    }
    // People who came up often with nothing written down.
    for g in store.memory_gaps(now).map_err(|e| e.to_string())? {
        let Some(eid) = g.entity_id else { continue };
        let mut f = HashMap::new();
        f.insert("person".into(), 1.0);
        f.insert("waiting".into(), 0.5);
        let mem = store.entity_memories(eid, 1).ok().and_then(|v| v.into_iter().next());
        cands.push(Cand { kind: "person", ref_id: eid, activity_id: mem.as_ref().map(|m| m.activity_id).unwrap_or(0), memory_id: mem.as_ref().map(|m| m.id).unwrap_or(0), title: g.text.clone(), features: f, bits: vec!["nothing written down".into()] });
    }
    // Threads: what ran through several days this week.
    for (e, days, count) in store.threads(now - 7 * 86_400_000, 3, 8).map_err(|e| e.to_string())? {
        let mut f = HashMap::new();
        f.insert("thread".into(), 1.0);
        f.insert("recurrence".into(), (days as f64 / 7.0).clamp(0.3, 1.0));
        let mem = store.entity_memories(e.id, 1).ok().and_then(|v| v.into_iter().next());
        cands.push(Cand { kind: "thread", ref_id: e.id, activity_id: mem.as_ref().map(|m| m.activity_id).unwrap_or(0), memory_id: mem.as_ref().map(|m| m.id).unwrap_or(0), title: format!("{} · {} memories this week", e.name, count), features: f, bits: vec![format!("{days} days running")] });
    }
    drop(store);
    // Calendar, next two days.
    if let Ok(events) = crate::engine::upcoming_events(state, 48) {
        for ev in events.into_iter().filter(|e| !e.all_day) {
            let hours = ((ev.start - now) / 3_600_000).max(0);
            let mut f = HashMap::new();
            f.insert("event".into(), 1.0);
            f.insert("soon".into(), (1.0 - hours as f64 / 48.0).clamp(0.1, 1.0));
            let bits = vec![if hours < 1 { "now".to_string() } else if hours < 24 { format!("in {hours} h") } else { "tomorrow".to_string() }, if ev.attendees.is_empty() { String::new() } else { format!("with {}", ev.attendees.iter().take(2).cloned().collect::<Vec<_>>().join(", ")) }].into_iter().filter(|b| !b.is_empty()).collect();
            cands.push(Cand { kind: "event", ref_id: 0, activity_id: 0, memory_id: 0, title: ev.title.clone(), features: f, bits });
        }
    }
    // Alignment and score.
    let mut scored: Vec<(f64, Cand)> = cands
        .into_iter()
        .map(|mut c| {
            if let Some(a) = align.get(&c.memory_id) {
                if *a >= 0.4 {
                    c.features.insert("align".into(), *a);
                    c.bits.push("near your why".into());
                }
            }
            let score: f64 = c.features.iter().map(|(k, v)| w.get(k).copied().unwrap_or(0.0) * v).sum();
            (score, c)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    // One entry per thing: a task and a date about the same thing collapse.
    let mut fresh: Vec<(String, i64, i64, String, String, f64, HashMap<String, f64>)> = Vec::new();
    for (score, c) in scored {
        if fresh.iter().any(|(_, _, _, t, _, _, _)| crate::engine::same_task(t, &c.title)) {
            continue;
        }
        let reason = c.bits.iter().filter(|b| !b.is_empty()).take(3).cloned().collect::<Vec<_>>().join(" · ");
        fresh.push((c.kind.to_string(), c.ref_id, c.activity_id, c.title, reason, score, c.features));
        if fresh.len() >= 7 {
            break;
        }
    }
    let n = fresh.len();
    crate::lock(&state.store).replace_signals(&day, &fresh, now).map_err(|e| e.to_string())?;
    Ok(n)
}

/// The person's verdict moves the weights a little: what they lift gains,
/// what they call noise loses. Bounded, so no feature ever disappears.
pub fn learn(state: &AppState, features: &HashMap<String, f64>, verdict: &str) {
    let delta = match verdict {
        "up" | "pinned" => 0.10,
        "noise" => -0.15,
        _ => return,
    };
    let mut s = crate::lock(&state.settings);
    let mut w = default_weights();
    for (k, v) in &s.signal_weights {
        w.insert(k.clone(), *v);
    }
    for (k, v) in features {
        let e = w.entry(k.clone()).or_insert(1.0);
        *e = (*e + delta * v).clamp(0.1, 6.0);
    }
    s.signal_weights = w;
    let _ = crate::lock(&state.store).save_settings(&s);
}

/// Score a day against the why and store it; returns (score, memories, near).
pub fn alignment_day(state: &AppState, ms: i64) -> Option<(f64, i64, i64)> {
    let align = alignment_map(state);
    if align.is_empty() {
        return None;
    }
    let (a, b) = day_bounds(ms);
    let ids = crate::lock(&state.store).memory_ids_between(a, b).ok()?;
    if ids.is_empty() {
        return None;
    }
    let scores: Vec<f64> = ids.iter().map(|id| align.get(id).copied().unwrap_or(0.0)).collect();
    let near = scores.iter().filter(|s| **s >= 0.45).count() as i64;
    let avg = scores.iter().sum::<f64>() / scores.len() as f64;
    let _ = crate::lock(&state.store).set_alignment_day(&day_of(ms), avg, ids.len() as i64, near);
    Some((avg, ids.len() as i64, near))
}

/// Two things the window in front connects to, from the co-occurrence graph.
pub fn connects_to(state: &AppState, title: &str) -> Vec<(String, i64)> {
    let store = crate::lock(&state.store);
    let Ok(named) = store.entities_named_in(title, 2) else { return vec![] };
    if named.is_empty() {
        return vec![];
    }
    let Ok(g) = store.graph(300, 1) else { return vec![] };
    let names: HashMap<i64, String> = g.nodes.iter().map(|n| (n.id, n.name.clone())).collect();
    let mut out: Vec<(String, i64)> = Vec::new();
    for e in &named {
        let mut edges: Vec<(i64, i64)> = g.edges.iter().filter_map(|x| if x.source == e.id { Some((x.target, x.weight)) } else if x.target == e.id { Some((x.source, x.weight)) } else { None }).collect();
        edges.sort_by(|a, b| b.1.cmp(&a.1));
        for (other, wgt) in edges.into_iter().take(2) {
            if let Some(n) = names.get(&other) {
                if !named.iter().any(|x| x.id == other) && !out.iter().any(|(m, _)| m == n) {
                    out.push((n.clone(), wgt));
                }
            }
        }
    }
    out.truncate(2);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn due_words_become_days() {
        let now = crate::capture::now_ms();
        assert_eq!(days_until_in("send the certificate today", now), Some(0));
        assert_eq!(days_until_in("call her tomorrow morning", now), Some(1));
        let d = days_until_in("confirm the vendor list by Monday", now).unwrap();
        assert!((1..=7).contains(&d));
        assert_eq!(days_until_in("nothing dated here", now), None);
    }
    #[test]
    fn stakes_need_money_words() {
        assert!(has_stakes(&["40 lakh".into()], "budget of ₹40 lakh"));
        assert!(!has_stakes(&["2500".into()], "2500 followers"));
        assert!(!has_stakes(&[], "₹ mentioned but no number extracted"));
    }
    #[test]
    fn weights_move_and_stay_bounded() {
        let mut w = default_weights();
        assert!(w["due"] > w["task"]);
        w.insert("noise".into(), 0.1);
        assert!(w.values().all(|v| *v >= 0.1 && *v <= 6.0));
    }
}
