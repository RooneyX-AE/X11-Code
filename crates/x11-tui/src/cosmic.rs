use std::fmt::Write as _;

#[derive(Debug, Clone, Copy)]
pub struct Star { pub x: u16, pub y: u16, pub phase: u8 }

#[derive(Debug, Clone)]
pub struct CosmicField {
    stars: Vec<Star>,
    frame: u64,
}

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
        Self { stars, frame: 0 }
    }

    pub fn tick(&mut self) { self.frame = self.frame.wrapping_add(1); }

    pub fn overlay(&self, width: u16, height: u16) -> String {
        let mut grid = vec![vec![' '; width as usize]; height as usize];
        for star in &self.stars {
            if star.x >= width || star.y >= height { continue; }
            let pulse = (self.frame.wrapping_add(star.phase as u64)) % 24;
            grid[star.y as usize][star.x as usize] = match pulse {
                0..=1 => '✦',
                2..=5 => '·',
                6..=18 => '.',
                _ => '·',
            };
        }
        let mut out = String::new();
        for row in grid {
            let _ = writeln!(&mut out, "{}", row.into_iter().collect::<String>());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_is_deterministic_for_same_size() {
        let a = CosmicField::new(80, 20);
        let b = CosmicField::new(80, 20);
        assert_eq!(a.overlay(80, 20), b.overlay(80, 20));
    }
}
