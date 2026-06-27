use std::fmt;

/// 3D entity location with rotation.
#[derive(Debug, Clone, Copy)]
pub struct EntityLocation {
    pub x: f64, pub y: f64, pub z: f64, pub pitch: f32, pub yaw: f32,
}

impl EntityLocation {
    pub fn new(x: f64, y: f64, z: f64, pitch: f32, yaw: f32) -> Self { Self { x, y, z, pitch, yaw } }
}

impl fmt::Display for EntityLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EntityLocation{{x={}, y={}, z={}, pitch={}, yaw={}}}", self.x, self.y, self.z, self.pitch, self.yaw)
    }
}

impl PartialEq for EntityLocation {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y && self.z == other.z && self.pitch == other.pitch && self.yaw == other.yaw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_equality() {
        let a = EntityLocation::new(1.0, 2.0, 3.0, 0.0, 90.0);
        let b = EntityLocation::new(1.0, 2.0, 3.0, 0.0, 90.0);
        let c = EntityLocation::new(10.0, 20.0, 30.0, 45.0, 180.0);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
