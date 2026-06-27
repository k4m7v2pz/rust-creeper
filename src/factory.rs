use crate::protocol::{GameVersion, UniversalProtocol};

/// Factory — creates protocol wrappers by version.
///
/// Sources:
///   1.11–1.16: TuxCoding fork (old Steveice10 MCProtocolLib)
///   1.17+:     GeyserMC/MCProtocolLib (embedded at reference/mcprotocollib/)
pub fn authenticate(game_version: GameVersion, username: &str) -> Result<Box<dyn UniversalProtocol>, String> {
    match game_version {
        GameVersion::V1_11 => Ok(Box::new(crate::protocol::v1_11::ProtocolWrapper::new(username))),
        GameVersion::V1_12 | GameVersion::V1_12_1 | GameVersion::V1_12_2 => Ok(Box::new(crate::protocol::v1_12::ProtocolWrapper::new(username))),
        GameVersion::V1_14_4 => Ok(Box::new(crate::protocol::v1_14::ProtocolWrapper::new(username))),
        GameVersion::V1_15 | GameVersion::V1_15_2 => Ok(Box::new(crate::protocol::v1_15::ProtocolWrapper::new(username))),
        GameVersion::V1_16_3 | GameVersion::V1_16_4 | GameVersion::V1_16_5 => Ok(Box::new(crate::protocol::v1_16::ProtocolWrapper::new(username))),
        GameVersion::V1_17_1 | GameVersion::V1_18 | GameVersion::V1_18_2
        | GameVersion::V1_19 | GameVersion::V1_19_2 | GameVersion::V1_19_4
        | GameVersion::V1_20 | GameVersion::V1_20_2 | GameVersion::V1_20_6
        | GameVersion::V1_21_0 | GameVersion::V1_21_1
        | GameVersion::V1_21_3 | GameVersion::V1_21_4 | GameVersion::V1_21_5
        | GameVersion::V1_21_6 | GameVersion::V1_21_7 => Ok(Box::new(crate::protocol::v1_21_1::ProtocolWrapper::new(username))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_authenticate_all_versions() {
        for (version, ver_str, expected) in [
            (GameVersion::V1_11, "1.11", GameVersion::V1_11),
            (GameVersion::V1_12, "1.12", GameVersion::V1_12_2),
            (GameVersion::V1_12_1, "1.12.1", GameVersion::V1_12_2),
            (GameVersion::V1_12_2, "1.12.2", GameVersion::V1_12_2),
            (GameVersion::V1_14_4, "1.14.4", GameVersion::V1_14_4),
            (GameVersion::V1_15, "1.15", GameVersion::V1_15_2),
            (GameVersion::V1_15_2, "1.15.2", GameVersion::V1_15_2),
            (GameVersion::V1_16_3, "1.16.3", GameVersion::V1_16_5),
            (GameVersion::V1_16_4, "1.16.4", GameVersion::V1_16_5),
            (GameVersion::V1_16_5, "1.16.5", GameVersion::V1_16_5),
            (GameVersion::V1_17_1, "1.17.1", GameVersion::V1_21_1),
            (GameVersion::V1_18, "1.18", GameVersion::V1_21_1),
            (GameVersion::V1_18_2, "1.18.2", GameVersion::V1_21_1),
            (GameVersion::V1_19, "1.19", GameVersion::V1_21_1),
            (GameVersion::V1_19_2, "1.19.2", GameVersion::V1_21_1),
            (GameVersion::V1_19_4, "1.19.4", GameVersion::V1_21_1),
            (GameVersion::V1_20, "1.20", GameVersion::V1_21_1),
            (GameVersion::V1_20_2, "1.20.2", GameVersion::V1_21_1),
            (GameVersion::V1_20_6, "1.20.6", GameVersion::V1_21_1),
            (GameVersion::V1_21_0, "1.21", GameVersion::V1_21_1),
            (GameVersion::V1_21_1, "1.21.1", GameVersion::V1_21_1),
            (GameVersion::V1_21_3, "1.21.3", GameVersion::V1_21_1),
            (GameVersion::V1_21_4, "1.21.4", GameVersion::V1_21_1),
            (GameVersion::V1_21_5, "1.21.5", GameVersion::V1_21_1),
            (GameVersion::V1_21_6, "1.21.6", GameVersion::V1_21_1),
            (GameVersion::V1_21_7, "1.21.7", GameVersion::V1_21_1),
        ] {
            let p = authenticate(version, "TestBot").unwrap();
            assert_eq!(p.game_version(), expected, "version mismatch for {ver_str}");
            assert_eq!(p.profile().name, "TestBot");
        }
    }
}
