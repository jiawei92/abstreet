use crate::raw::OriginalRoad;
use crate::{osm, AccessRestrictions, Direction, LaneType, Map, PermanentMapEdits, Road};
use geom::Speed;
use serde::Deserialize;
use serde_json::Value;

// When the PermanentMapEdits format changes, add a transformation here to automatically convert
// edits written with the old format.
//
// This problem is often solved with something like protocol buffers, but the resulting proto
// usually winds up with permanent legacy fields, unless the changes are purely additive. For
// example, protobufs wouldn't have helped with the fix_intersection_ids problem. Explicit
// transformation is easier!
pub fn upgrade(mut value: Value, map: &Map) -> Result<PermanentMapEdits, String> {
    // c46a74f10f4f1976a48aa8642ac11717d74b262c added an explicit version field. There are a few
    // changes before that.
    if value.get("version").is_none() {
        // I don't remember the previous schema change before this. If someone files a bug and has
        // an older file, can add support for it then.
        fix_offset(&mut value);
        fix_intersection_ids(&mut value);

        value
            .as_object_mut()
            .unwrap()
            .insert("version".to_string(), Value::Number(0.into()));
    }
    if value["version"] == Value::Number(0.into()) {
        fix_road_direction(&mut value);
        value
            .as_object_mut()
            .unwrap()
            .insert("version".to_string(), Value::Number(1.into()));
    }
    if value["version"] == Value::Number(1.into()) {
        fix_old_lane_cmds(&mut value, map)?;
        value
            .as_object_mut()
            .unwrap()
            .insert("version".to_string(), Value::Number(2.into()));
    }

    abstutil::from_json(&value.to_string().into_bytes()).map_err(|x| x.to_string())
}

// Recursively walks the entire JSON object. Will call transform on all of the map objects. If the
// callback returns true, won't recurse into that map.
fn walk<F: Fn(&mut serde_json::Map<String, Value>) -> bool>(value: &mut Value, transform: &F) {
    match value {
        Value::Array(list) => {
            for x in list {
                walk(x, transform);
            }
        }
        Value::Object(map) => {
            if !(transform)(map) {
                for x in map.values_mut() {
                    walk(x, transform);
                }
            }
        }
        _ => {}
    }
}

// eee179ce8a6c1e6133dc212b73c3f79b11603e82 added an offset_seconds field
fn fix_offset(value: &mut Value) {
    walk(value, &|map| {
        if map.len() == 1 && map.contains_key("TrafficSignal") {
            let ts = map
                .get_mut("TrafficSignal")
                .unwrap()
                .as_object_mut()
                .unwrap();
            if ts.get("offset_seconds").is_none() {
                ts.insert("offset_seconds".to_string(), Value::Number(0.into()));
            }
            true
        } else {
            false
        }
    })
}

// 11cefb118ab353d2e7fa5dceaab614a9b775e6ec changed { "osm_node_id": 123 } to just 123
fn fix_intersection_ids(value: &mut Value) {
    match value {
        Value::Array(list) => {
            for x in list {
                fix_intersection_ids(x);
            }
        }
        Value::Object(map) => {
            if map.len() == 1 && map.contains_key("osm_node_id") {
                *value = Value::Number(map["osm_node_id"].as_i64().unwrap().into());
            } else {
                for x in map.values_mut() {
                    fix_intersection_ids(x);
                }
            }
        }
        _ => {}
    }
}

// b137735e019adbe0f2a7372a579aa987f8496e19 changed direction from a boolean to an enum.
fn fix_road_direction(value: &mut Value) {
    walk(value, &|map| {
        if map.contains_key("num_fwd") {
            map.insert(
                "dir".to_string(),
                if map["fwd"].as_bool().unwrap() {
                    "Fwd".into()
                } else {
                    "Back".into()
                },
            );
            true
        } else {
            false
        }
    });
}

// b6ab06d51a3b22702b66db296ed4dfd27e8403a0 (and adjacent commits) removed some commands that
// target a single lane in favor of a consolidated ChangeRoad.
fn fix_old_lane_cmds(value: &mut Value, map: &Map) -> Result<(), String> {
    // TODO Can we assume map is in its original state? I don't think so... it may have edits
    // applied, right?
    for cmd in value.as_object_mut().unwrap()["commands"]
        .as_array_mut()
        .unwrap()
    {
        let cmd = cmd.as_object_mut().unwrap();
        if let Some(obj) = cmd.remove("ChangeLaneType") {
            let obj: ChangeLaneType = serde_json::from_value(obj).unwrap();
            let (r, idx) = obj.id.lookup(map)?;
            if r.lanes_ltr()[idx].2 != obj.orig_lt {
                return Err(format!("{:?} lane type has changed", obj));
            }
            let replace = map
                .edit_road_cmd(r.id, |new| {
                    new.lanes_ltr[idx].0 = obj.lt;
                })
                .to_perma(map);
            cmd.insert(
                "ChangeRoad".to_string(),
                serde_json::to_value(replace).unwrap(),
            );
        }
    }
    Ok(())
}

// These're old structs used in fix_old_lane_cmds.
#[derive(Debug, Deserialize)]
struct OriginalLane {
    parent: OriginalRoad,
    num_fwd: usize,
    num_back: usize,
    dir: Direction,
    idx: usize,
}
#[derive(Debug, Deserialize)]
struct ChangeLaneType {
    id: OriginalLane,
    lt: LaneType,
    orig_lt: LaneType,
}
#[derive(Debug, Deserialize)]
struct ReverseLane {
    l: OriginalLane,
    // New intended dst_i
    dst_i: osm::NodeID,
}
#[derive(Debug, Deserialize)]
struct ChangeSpeedLimit {
    id: OriginalRoad,
    new: Speed,
    old: Speed,
}
#[derive(Debug, Deserialize)]
struct ChangeAccessRestrictions {
    id: OriginalRoad,
    new: AccessRestrictions,
    old: AccessRestrictions,
}

impl OriginalLane {
    fn lookup<'a>(&self, map: &'a Map) -> Result<(&'a Road, usize), String> {
        let r = map.get_r(map.find_r_by_osm_id(self.parent)?);
        let current_fwd = r.children_forwards();
        let current_back = r.children_backwards();
        if current_fwd.len() != self.num_fwd || current_back.len() != self.num_back {
            return Err(format!(
                "number of lanes in {} is ({} fwd, {} back) now, but ({}, {}) in the edits",
                r.orig_id,
                current_fwd.len(),
                current_back.len(),
                self.num_fwd,
                self.num_back
            ));
        }
        let l = if self.dir == Direction::Fwd {
            current_fwd[self.idx].0
        } else {
            current_back[self.idx].0
        };
        Ok((r, r.offset(l)))
    }
}
