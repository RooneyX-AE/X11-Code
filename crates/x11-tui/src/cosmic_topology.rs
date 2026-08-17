use std::fmt::Write as _;

use crate::cosmic_state::CosmicTopology;

fn glyph_for_state(state: &str, progress: u8) -> char {
    let state = state.to_ascii_lowercase();
    if state.contains("fail") || state.contains("cancel") { '×' }
    else if state.contains("verify") { '◎' }
    else if state.contains("complete") || progress >= 100 { '◆' }
    else if state.contains("resolve") { '◈' }
    else if state.contains("conflict") { '✶' }
    else if state.contains("blocked") { '□' }
    else if progress >= 75 { '●' }
    else if progress >= 40 { '◉' }
    else { '○' }
}

fn short_label(value: &str, max: usize) -> String {
    let mut out = value.chars().take(max).collect::<String>();
    if value.chars().count() > max { out.push('…'); }
    out
}

pub fn render_topology(topology: &CosmicTopology, width: u16, height: u16, frame: u64) -> String {
    if width == 0 || height == 0 { return String::new(); }
    let mut grid = vec![vec![' '; width as usize]; height as usize];
    let cx = (width / 2) as i32;
    let cy = (height / 2) as i32;
    let base_radius = (width.min(height).max(8) / 4).max(3) as i32;

    let agents = topology.agents.values().collect::<Vec<_>>();
    for (i, agent) in agents.iter().enumerate() {
        let angle = ((i as u64 * 53 + frame / 6) % 360) as f64;
        let radius = base_radius + (i as i32 % 3) * 3;
        let x = cx + ((angle.to_radians().cos() * radius as f64) as i32);
        let y = cy + ((angle.to_radians().sin() * (radius as f64 * 0.45)) as i32);
        if x < 0 || y < 0 || (x as u16) >= width || (y as u16) >= height { continue; }
        let glyph = glyph_for_state(&agent.state, agent.progress);
        grid[y as usize][x as usize] = glyph;
        if x + 1 < width as i32 {
            grid[y as usize][(x + 1) as usize] = match agent.progress {
                0..=24 => '·', 25..=49 => '∘', 50..=74 => '∙', 75..=99 => '•', _ => '●',
            };
        }
    }

    let tasks = topology.tasks.values().collect::<Vec<_>>();
    for (i, task) in tasks.iter().enumerate() {
        let angle = ((i as u64 * 71 + frame / 9) % 360) as f64;
        let radius = base_radius / 2 + 2 + (i as i32 % 4);
        let x = cx + ((angle.to_radians().cos() * radius as f64) as i32);
        let y = cy + ((angle.to_radians().sin() * (radius as f64 * 0.45)) as i32);
        if x < 0 || y < 0 || (x as u16) >= width || (y as u16) >= height { continue; }
        let glyph = glyph_for_state(&task.state, task.progress);
        if grid[y as usize][x as usize] == ' ' { grid[y as usize][x as usize] = glyph; }
        let progress_glyph = if task.progress >= 100 { '●' } else if task.progress >= 50 { '◐' } else { '○' };
        if x + 1 < width as i32 && grid[y as usize][(x + 1) as usize] == ' ' {
            grid[y as usize][(x + 1) as usize] = progress_glyph;
        }
        // Compact task label in the upper-left margin. Labels are derived from task IDs only.
        if i < 6 {
            let label = format!("{} {}", short_label(&task.id, 12), task.progress);
            let row = 1 + i;
            for (col, ch) in label.chars().enumerate() {
                if col + 1 < width as usize && row < height as usize { grid[row][col] = ch; }
            }
        }
    }

    for (i, _) in topology.conflicts.iter().enumerate() {
        let pulse = ((frame / 2 + i as u64 * 3) % 12) as i32;
        let radius = pulse.saturating_sub(1);
        for (x, y) in [
            (cx + radius, cy), (cx - radius, cy),
            (cx, cy + radius / 2), (cx, cy - radius / 2),
        ] {
            if x >= 0 && y >= 0 && (x as u16) < width && (y as u16) < height { grid[y as usize][x as usize] = '✶'; }
        }
    }

    let resolving = tasks.iter().any(|t| {
        let s = t.state.to_ascii_lowercase();
        s.contains("resolve") || s.contains("resolver")
    });
    if resolving && width > 4 && height > 2 {
        let radius = ((frame / 3) % (width.min(height).max(6) as u64 / 2 + 1)) as i32;
        for (x, y) in [
            (cx + radius, cy), (cx - radius, cy),
            (cx, cy + radius / 2), (cx, cy - radius / 2),
        ] {
            if x >= 0 && y >= 0 && (x as u16) < width && (y as u16) < height { grid[y as usize][x as usize] = '◈'; }
        }
    }

    // Recent runtime timeline is rendered as a compact strip at the bottom.
    if height > 2 {
        let timeline_y = height as usize - 2;
        let mut timeline = String::from("│ ");
        for entry in topology.timeline.iter().rev().take(5).rev() {
            let marker = short_label(&entry.kind, 10);
            if !timeline.ends_with("│ ") { timeline.push_str(" · "); }
            timeline.push_str(&marker);
        }
        for (col, ch) in timeline.chars().take(width as usize).enumerate() { grid[timeline_y][col] = ch; }
    }

    let mut out = String::new();
    for row in grid { let _ = writeln!(&mut out, "{}", row.into_iter().collect::<String>()); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use x11_agent::swarm_events::{SwarmEvent, SwarmEventKind};

    #[test]
    fn topology_render_is_deterministic_for_same_frame() {
        let mut topology = CosmicTopology::default();
        topology.apply(&SwarmEvent::new(Uuid::new_v4(), SwarmEventKind::TaskStarted)
            .task("task").agent("agent").progress(25).state("running"));
        assert_eq!(render_topology(&topology, 60, 20, 10), render_topology(&topology, 60, 20, 10));
    }

    #[test]
    fn progress_changes_task_projection() {
        let swarm = Uuid::new_v4();
        let mut low = CosmicTopology::default();
        low.apply(&SwarmEvent::new(swarm, SwarmEventKind::TaskStarted)
            .task("task").agent("agent").progress(10).state("running"));
        let mut high = CosmicTopology::default();
        high.apply(&SwarmEvent::new(swarm, SwarmEventKind::TaskStarted)
            .task("task").agent("agent").progress(90).state("running"));
        assert_ne!(render_topology(&low, 60, 20, 12), render_topology(&high, 60, 20, 12));
    }

    #[test]
    fn conflict_and_resolver_add_visual_markers() {
        let swarm = Uuid::new_v4();
        let mut topology = CosmicTopology::default();
        topology.apply(&SwarmEvent::new(swarm, SwarmEventKind::ConflictDetected)
            .task("task").evidence("conflict"));
        assert!(render_topology(&topology, 60, 20, 12).contains('✶'));
        topology.apply(&SwarmEvent::new(swarm, SwarmEventKind::ResolverStarted)
            .task("task").state("resolving"));
        let rendered = render_topology(&topology, 60, 20, 12);
        assert!(rendered.contains('◈'));
        assert!(rendered.contains("task"));
    }

    #[test]
    fn timeline_is_visible_and_bounded_by_viewport() {
        let swarm = Uuid::new_v4();
        let mut topology = CosmicTopology::default();
        for _ in 0..20 {
            topology.apply(&SwarmEvent::new(swarm, SwarmEventKind::TaskQueued).task("task"));
        }
        let rendered = render_topology(&topology, 40, 10, 5);
        assert_eq!(rendered.lines().count(), 10);
        assert!(rendered.contains("TaskQueued"));
    }
}
