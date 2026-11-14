//! Surface fuel models. Loads are the published Anderson (1982, GTR INT-122) /
//! Albini (1976) "13 fuel models" in their original US customary units, which
//! is what the Rothermel (1972) equations expect.
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct FuelModel {
    pub code: &'static str,
    pub name: &'static str,
    /// Oven-dry loads in tons/acre: 1-h, 10-h, 100-h dead, live.
    pub load_tpa: [f64; 4],
    /// Surface-area-to-volume of 1-h dead fuel, 1/ft.
    pub sav_1h_per_ft: f64,
    /// Surface-area-to-volume of live fuel, 1/ft.
    pub sav_live_per_ft: f64,
    pub depth_ft: f64,
    /// Dead fuel moisture of extinction, percent.
    pub mx_dead_pct: f64,
    /// Low heat content, BTU/lb.
    pub heat_btu_per_lb: f64,
    /// Wind adjustment factor from 20-ft to midflame wind, unsheltered values
    /// (Albini & Baughman 1979; Andrews 2012). Unsheltered is the conservative
    /// (faster) choice for planning; canopy sheltering would reduce spread.
    pub wind_adjustment: f64,
}

pub const SAV_10H_PER_FT: f64 = 109.0;
pub const SAV_100H_PER_FT: f64 = 30.0;

const fn fm(
    code: &'static str,
    name: &'static str,
    load_tpa: [f64; 4],
    sav_1h_per_ft: f64,
    depth_ft: f64,
    mx_dead_pct: f64,
    wind_adjustment: f64,
) -> FuelModel {
    FuelModel {
        code,
        name,
        load_tpa,
        sav_1h_per_ft,
        sav_live_per_ft: 1500.0,
        depth_ft,
        mx_dead_pct,
        heat_btu_per_lb: 8000.0,
        wind_adjustment,
    }
}

/// Anderson's 13 standard models (loads, depth and extinction moisture per GTR INT-122;
/// 1-h SAV per Albini 1976 as used in BehavePlus).
pub const ANDERSON_13: [FuelModel; 13] = [
    fm(
        "FM1",
        "Short grass",
        [0.74, 0.0, 0.0, 0.0],
        3500.0,
        1.0,
        12.0,
        0.36,
    ),
    fm(
        "FM2",
        "Timber (grass and understory)",
        [2.0, 1.0, 0.5, 0.5],
        3000.0,
        1.0,
        15.0,
        0.36,
    ),
    fm(
        "FM3",
        "Tall grass",
        [3.01, 0.0, 0.0, 0.0],
        1500.0,
        2.5,
        25.0,
        0.44,
    ),
    fm(
        "FM4",
        "Chaparral / tall dense shrub",
        [5.01, 4.01, 2.0, 5.01],
        2000.0,
        6.0,
        20.0,
        0.55,
    ),
    fm(
        "FM5",
        "Brush",
        [1.0, 0.5, 0.0, 2.0],
        2000.0,
        2.0,
        20.0,
        0.42,
    ),
    fm(
        "FM6",
        "Dormant brush",
        [1.5, 2.5, 2.0, 0.0],
        1750.0,
        2.5,
        25.0,
        0.44,
    ),
    fm(
        "FM7",
        "Southern rough",
        [1.13, 1.87, 1.5, 0.37],
        1750.0,
        2.5,
        40.0,
        0.44,
    ),
    fm(
        "FM8",
        "Closed timber litter",
        [1.5, 1.0, 2.5, 0.0],
        2000.0,
        0.2,
        30.0,
        0.28,
    ),
    fm(
        "FM9",
        "Hardwood litter",
        [2.92, 0.41, 0.15, 0.0],
        2500.0,
        0.2,
        25.0,
        0.28,
    ),
    fm(
        "FM10",
        "Timber (litter and understory)",
        [3.01, 2.0, 5.01, 2.0],
        2000.0,
        1.0,
        25.0,
        0.36,
    ),
    fm(
        "FM11",
        "Light logging slash",
        [1.5, 4.51, 5.51, 0.0],
        1500.0,
        1.0,
        15.0,
        0.36,
    ),
    fm(
        "FM12",
        "Medium logging slash",
        [4.01, 14.03, 16.53, 0.0],
        1500.0,
        2.3,
        20.0,
        0.43,
    ),
    fm(
        "FM13",
        "Heavy logging slash",
        [7.01, 23.04, 28.05, 0.0],
        1500.0,
        3.0,
        25.0,
        0.46,
    ),
];

pub fn anderson(number: u8) -> Option<&'static FuelModel> {
    ANDERSON_13.get(usize::from(number).checked_sub(1)?)
}

/// Land-cover classes relevant to Portuguese wildland. Each maps to an
/// approximate standard model; these are not the calibrated Portuguese custom
/// models of Fernandes et al. and are labelled as approximations everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LandCover {
    Eucalyptus,
    MaritimePine,
    Shrubland,
    TallShrubland,
    Grassland,
    Agriculture,
    Montado,
    Broadleaf,
    MixedForest,
    NonBurnable,
}

impl LandCover {
    pub fn fuel(self) -> Option<&'static FuelModel> {
        match self {
            // Eucalypt stands with litter, bark and shrub understory.
            LandCover::Eucalyptus => anderson(10),
            // Pinus pinaster with shrub understory behaves close to a southern rough.
            LandCover::MaritimePine => anderson(7),
            LandCover::Shrubland => anderson(6),
            LandCover::TallShrubland => anderson(4),
            LandCover::Grassland => anderson(1),
            LandCover::Agriculture => anderson(1),
            LandCover::Montado => anderson(2),
            LandCover::Broadleaf => anderson(9),
            LandCover::MixedForest => anderson(10),
            LandCover::NonBurnable => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            LandCover::Eucalyptus => "eucalyptus",
            LandCover::MaritimePine => "maritime pine",
            LandCover::Shrubland => "shrubland (mato)",
            LandCover::TallShrubland => "tall dense shrubland",
            LandCover::Grassland => "grass / pasture",
            LandCover::Agriculture => "agriculture",
            LandCover::Montado => "montado (oak woodland)",
            LandCover::Broadleaf => "broadleaf woodland",
            LandCover::MixedForest => "mixed forest",
            LandCover::NonBurnable => "non-burnable",
        }
    }
    pub const ALL: [LandCover; 10] = [
        LandCover::Eucalyptus,
        LandCover::MaritimePine,
        LandCover::Shrubland,
        LandCover::TallShrubland,
        LandCover::Grassland,
        LandCover::Agriculture,
        LandCover::Montado,
        LandCover::Broadleaf,
        LandCover::MixedForest,
        LandCover::NonBurnable,
    ];
    pub fn index(self) -> u8 {
        LandCover::ALL.iter().position(|c| *c == self).unwrap_or(9) as u8
    }
    pub fn from_index(i: u8) -> LandCover {
        LandCover::ALL
            .get(usize::from(i))
            .copied()
            .unwrap_or(LandCover::NonBurnable)
    }
}

/// Map Portuguese/English vegetation words (as extracted from reports) to a class.
pub fn land_cover_from_text(text: &str) -> Option<LandCover> {
    let t = text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if has(&["eucalip", "eucalyp"]) {
        Some(LandCover::Eucalyptus)
    } else if has(&["pinh", "pinus", "pine", "pinheir"]) {
        Some(LandCover::MaritimePine)
    } else if has(&["mato denso", "matos densos", "dense shrub", "mato alto"]) {
        Some(LandCover::TallShrubland)
    } else if has(&[
        "mato", "tojo", "urze", "giesta", "esteva", "shrub", "scrub", "heath",
    ]) {
        Some(LandCover::Shrubland)
    } else if has(&["montado", "sobreir", "azinh", "cork"]) {
        Some(LandCover::Montado)
    } else if has(&[
        "carvalh",
        "castanh",
        "folhosa",
        "oak",
        "chestnut",
        "broadleaf",
    ]) {
        Some(LandCover::Broadleaf)
    } else if has(&[
        "agríc", "agric", "cultivo", "restolho", "cereal", "vinha", "olival", "farm",
    ]) {
        Some(LandCover::Agriculture)
    } else if has(&["erva", "pasto", "pastagem", "prado", "grass"]) {
        Some(LandCover::Grassland)
    } else if has(&["floresta", "povoamento", "forest", "mata"]) {
        Some(LandCover::MixedForest)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_rows_are_transcribed() {
        let fm4 = anderson(4).unwrap();
        assert_eq!(fm4.load_tpa, [5.01, 4.01, 2.0, 5.01]);
        assert_eq!((fm4.depth_ft, fm4.mx_dead_pct), (6.0, 20.0));
        let fm10 = anderson(10).unwrap();
        assert_eq!(fm10.load_tpa, [3.01, 2.0, 5.01, 2.0]);
        assert_eq!((fm10.depth_ft, fm10.mx_dead_pct), (1.0, 25.0));
        let fm13 = anderson(13).unwrap();
        assert_eq!(fm13.load_tpa, [7.01, 23.04, 28.05, 0.0]);
        assert!(anderson(0).is_none() && anderson(14).is_none());
    }

    #[test]
    fn portuguese_vegetation_words_map_to_classes() {
        assert_eq!(
            land_cover_from_text("eucaliptal"),
            Some(LandCover::Eucalyptus)
        );
        assert_eq!(
            land_cover_from_text("Pinhal bravo"),
            Some(LandCover::MaritimePine)
        );
        assert_eq!(
            land_cover_from_text("zona de mato"),
            Some(LandCover::Shrubland)
        );
        assert_eq!(
            land_cover_from_text("povoamento florestal"),
            Some(LandCover::MixedForest)
        );
        assert_eq!(land_cover_from_text("estrada"), None);
        assert!(LandCover::NonBurnable.fuel().is_none());
        for c in LandCover::ALL {
            assert_eq!(LandCover::from_index(c.index()), c);
        }
    }
}
