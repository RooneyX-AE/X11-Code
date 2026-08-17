use std::fmt::Write as _;
use crate::cosmic_state::{CosmicPhase, CosmicTopology};
#[path = "cosmic_topology.rs"]
pub mod topology_renderer;
#[derive(Debug, Clone, Copy)]
pub struct Star { pub x: u16, pub y: u16, pub phase: u8 }
#[derive(Debug, Clone)]
pub struct CosmicField { stars: Vec<Star>, frame: u64, phase: CosmicPhase, topology: CosmicTopology }
impl CosmicField {
    pub fn new(width: u16, height: u16) -> Self {
        let mut stars = Vec::new();
        let count = ((width as usize * height as usize) / 90).clamp(12, 140);
        let mut seed = width as u64 * 1_315_423_911 ^ height as u64 * 2_654_435_761;
        for _ in 0..count {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x = if width == 0 { 0 } else { (seed % width as u64) as u16 };
            seed = seed.rotate_left(13);
            let y = if height == 0 { 0 } else { (seed % height as u64) as u16 };
            stars.push(Star { x, y, phase: (seed & 0xff) as u8 });
        }
        Self { stars, frame: 0, phase: CosmicPhase::Idle, topology: CosmicTopology::default() }
    }
    pub fn tick(&mut self) { self.frame = self.frame.wrapping_add(1); }
    pub fn set_phase(&mut self, phase: CosmicPhase) { self.phase = phase; }
    pub fn phase(&self) -> CosmicPhase { self.phase }
    pub fn apply_swarm_event(&mut self, event: &x11_agent::swarm_events::SwarmEvent) { self.topology.apply(event); self.phase = CosmicPhase::from_event(&event.kind); self.tick(); }
    pub fn topology(&self) -> &CosmicTopology { &self.topology }
    pub fn overlay(&self, width: u16, height: u16) -> String {
        let mut grid = vec![vec![' '; width as usize]; height as usize];
        let density = match self.phase { CosmicPhase::Idle => 1, CosmicPhase::Running => 1, CosmicPhase::Conflict => 2, CosmicPhase::Resolving => 2, CosmicPhase::Verifying => 1, CosmicPhase::Completed => 1, CosmicPhase::Failed => 1 };
        for star in &self.stars {
            if star.x >= width || star.y >= height { continue; }
            let pulse = (self.frame.wrapping_add(star.phase as u64)) % 24;
            let glyph = match self.phase {
                CosmicPhase::Idle => match pulse { 0..=1 => '·', _ => '.' },
                CosmicPhase::Running => match pulse { 0..=1 => '✦', 2..=5 => '·', 6..=18 => '.', _ => '·' },
                CosmicPhase::Conflict => match (pulse + density as u64) % 12 { 0..=2 => '✶', 3..=5 => '×', _ => '.' },
                CosmicPhase::Resolving => match pulse % 10 { 0..=2 => '✦', 3..=5 => '·', _ => '.' },
                CosmicPhase::Verifying => match pulse % 8 { 0..=2 => '○', 3..=4 => '·', _ => '.' },
                CosmicPhase::Completed => match pulse % 16 { 0..=3 => '✦', 4..=8 => '·', _ => '.' },
                CosmicPhase::Failed => match pulse % 12 { 0..=2 => '×', 3..=5 => '·', _ => '.' },
            };
            grid[star.y as usize][star.x as usize] = glyph;
        }
        if matches!(self.phase, CosmicPhase::Resolving | CosmicPhase::Verifying) && width > 4 && height > 2 {
            let cx = width / 2; let cy = height / 2;
            let radius = ((self.frame / 2) % (width.min(height).max(6) as u64 / 2 + 1)) as i32;
            for (x, y) in [(cx as i32 + radius, cy as i32), (cx as i32 - radius, cy as i32), (cx as i32, cy as i32 + radius), (cx as i32, cy as i32 - radius)] {
                if x >= 0 && y >= 0 && (x as u16) < width && (y as u16) < height { grid[y as usize][x as usize] = if self.phase == CosmicPhase::Resolving { '◉' } else { '○' }; }
            }
        }
        let topology = topology_renderer::render_topology(&self.topology, width, height, self.frame);
        for (y, row) in topology.lines().enumerate() {
            if y >= grid.len() { break; }
            for (x, glyph) in row.chars().enumerate() { if x >= grid[y].len() { break; } if glyph != ' ' { grid[y][x] = glyph; } }
        }
        let mut out = String::new();
        for row in grid { let _ = writeln!(&mut out, "{}", row.into_iter().collect::<String>()); }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*; use uuid::Uuid;
    #[test] fn field_is_deterministic_for_same_size_and_phase() { let mut a=CosmicField::new(80,20); let mut b=CosmicField::new(80,20); a.set_phase(CosmicPhase::Running); b.set_phase(CosmicPhase::Running); assert_eq!(a.overlay(80,20), b.overlay(80,20)); }
    #[test] fn phase_changes_visual_projection() { let mut field=CosmicField::new(40,12); field.set_phase(CosmicPhase::Running); let running=field.overlay(40,12); field.set_phase(CosmicPhase::Conflict); assert_ne!(running,field.overlay(40,12)); }
    #[test] fn frame_changes_animation() { let mut field=CosmicField::new(40,12); field.set_phase(CosmicPhase::Running); let before=field.overlay(40,12); field.tick(); assert_ne!(before,field.overlay(40,12)); }
    #[test] fn swarm_event_updates_topology_and_phase() { let mut field=CosmicField::new(60,20); let swarm=Uuid::new_v4(); field.apply_swarm_event(&x11_agent::swarm_events::SwarmEvent::new(swarm,x11_agent::swarm_events::SwarmEventKind::TaskStarted).task("task-1").agent("agent-1").progress(25).state("running")); assert_eq!(field.phase(),CosmicPhase::Running); assert_eq!(field.topology().tasks["task-1"].progress,25); assert!(field.overlay(60,20).contains('◉')); }
}
