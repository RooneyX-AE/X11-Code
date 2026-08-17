use std::fmt::Write as _;
use crate::cosmic_state::CosmicTopology;

pub fn render_topology(topology: &CosmicTopology, width: u16, height: u16, frame: u64) -> String {
    let mut grid = vec![vec![' '; width as usize]; height as usize];
    if width == 0 || height == 0 { return String::new(); }
    let cx = (width / 2) as i32;
    let cy = (height / 2) as i32;
    let radius = (width.min(height).max(8) / 4).max(3) as i32;
    let agents = topology.agents.values().collect::<Vec<_>>();
    for (i, agent) in agents.iter().enumerate() {
        let angle = ((i as u64 * 37 + frame / 4) % 360) as f64;
        let r = radius + (i as i32 % 3) * 2;
        let x = cx + ((angle.to_radians().cos() * r as f64) as i32);
        let y = cy + ((angle.to_radians().sin() * (r as f64 * 0.45)) as i32);
        if x >= 0 && y >= 0 && (x as u16) < width && (y as u16) < height {
            grid[y as usize][x as usize] = if agent.state.contains("failed") { '×' } else if agent.state.contains("verify") { '◎' } else { '◉' };
        }
    }
    for (i, conflict) in topology.conflicts.iter().enumerate() {
        let pulse = ((frame / 3 + i as u64) % 8) as i32;
        let x = cx + pulse - 3;
        if x >= 0 && (x as u16) < width && cy >= 0 && (cy as u16) < height { grid[cy as usize][x as usize] = '✶'; }
        let _ = conflict;
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
    use crate::cosmic_state::CosmicTopology;

    #[test]
    fn topology_render_is_deterministic_for_same_frame() {
        let mut topology = CosmicTopology::default();
        topology.apply(&SwarmEvent::new(Uuid::new_v4(), SwarmEventKind::TaskStarted).task("task").agent("agent").progress(25).state("running"));
        assert_eq!(render_topology(&topology, 60, 20, 10), render_topology(&topology, 60, 20, 10));
    }
}
