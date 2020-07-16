use crate::utils::{download, download_kml, osmconvert};

fn input() {
    download(
        "input/berlin/osm/berlin-latest.osm.pbf",
        "http://download.geofabrik.de/europe/germany/berlin-latest.osm.pbf",
    );

    let bounds = geom::GPSBounds::from(
        geom::LonLat::read_osmosis_polygon(abstutil::path(
            "input/berlin/polygons/berlin_center.poly",
        ))
        .unwrap(),
    );
    // From https://data.technologiestiftung-berlin.de/dataset/lor_planungsgraeume/en
    download_kml(
        "input/berlin/planning_areas.bin",
        "https://tsb-opendata.s3.eu-central-1.amazonaws.com/lor_planungsgraeume/lor_planungsraeume.kml",
        &bounds,
        // Keep partly out-of-bounds polygons
        false,
    );
}

pub fn osm_to_raw(name: &str) {
    input();
    osmconvert(
        "input/berlin/osm/berlin-latest.osm.pbf",
        format!("input/berlin/polygons/{}.poly", name),
        format!("input/berlin/osm/{}.osm", name),
    );

    println!("- Running convert_osm");
    let map = convert_osm::convert(
        convert_osm::Options {
            osm_input: abstutil::path(format!("input/berlin/osm/{}.osm", name)),
            city_name: "berlin".to_string(),
            name: name.to_string(),

            clip: Some(abstutil::path(format!(
                "input/berlin/polygons/{}.poly",
                name
            ))),
            map_config: map_model::MapConfig {
                driving_side: map_model::raw::DrivingSide::Right,
                bikes_can_use_bus_lanes: true,
            },

            onstreet_parking: convert_osm::OnstreetParking::JustOSM,
            public_offstreet_parking: convert_osm::PublicOffstreetParking::None,
            private_offstreet_parking: convert_osm::PrivateOffstreetParking::FixedPerBldg(3),
            elevation: None,
        },
        &mut abstutil::Timer::throwaway(),
    );
    let output = abstutil::path(format!("input/raw_maps/{}.bin", name));
    println!("- Saving {}", output);
    abstutil::write_binary(output, &map);
}