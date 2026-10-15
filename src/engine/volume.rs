//! Volume and mute, set directly through PipeWire the way WirePlumber's
//! mixer API (and so `wpctl` and the Plasma applet) does it: hardware devices
//! get the volume on their active route, so it lands on the real mixer
//! control and WirePlumber remembers it per port; everything else (apps,
//! buses, virtual devices) gets it on the node itself.

use pipewire as pw;
use pw::spa;
use pw::spa::pod::{Object, Property, Value, ValueArray};
use pw::spa::utils::SpaTypes;

#[derive(Debug, Clone, Copy)]
pub enum Change {
    /// On the cubic scale used by volume sliders; 1.0 is 100%.
    Volume(f64),
    Mute(bool),
}

/// An active route of a device: what is needed to address it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    pub index: i32,
    pub channels: usize,
}

/// A Props object for `change`. `id` is the param it travels in: Props on a
/// node, Route inside a route.
fn props(change: Change, channels: usize, id: spa::param::ParamType) -> Value {
    let property = match change {
        Change::Volume(v) => {
            let linear = v.clamp(0.0, 1.5).powi(3) as f32;
            Property::new(
                spa::sys::SPA_PROP_channelVolumes,
                Value::ValueArray(ValueArray::Float(vec![linear; channels.max(1)])),
            )
        }
        Change::Mute(m) => Property::new(spa::sys::SPA_PROP_mute, Value::Bool(m)),
    };
    Value::Object(Object { type_: SpaTypes::ObjectParamProps.as_raw(), id: id.as_raw(), properties: vec![property] })
}

fn serialize(value: &Value) -> Option<Vec<u8>> {
    spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), value)
        .ok()
        .map(|(cursor, _)| cursor.into_inner())
}

/// Props param for a node.
pub fn node_param(change: Change, channels: usize) -> Option<Vec<u8>> {
    serialize(&props(change, channels, spa::param::ParamType::Props))
}

/// Route param that changes the volume of a device's active route.
/// `device` is the node's `card.profile.device`.
pub fn route_param(route: Route, device: i32, change: Change) -> Option<Vec<u8>> {
    let property = |key, value| Property::new(key, value);
    serialize(&Value::Object(Object {
        type_: SpaTypes::ObjectParamRoute.as_raw(),
        id: spa::param::ParamType::Route.as_raw(),
        properties: vec![
            property(spa::sys::SPA_PARAM_ROUTE_index, Value::Int(route.index)),
            property(spa::sys::SPA_PARAM_ROUTE_device, Value::Int(device)),
            property(
                spa::sys::SPA_PARAM_ROUTE_props,
                props(change, route.channels, spa::param::ParamType::Route),
            ),
            // Let WirePlumber store it for this port.
            property(spa::sys::SPA_PARAM_ROUTE_save, Value::Bool(true)),
        ],
    }))
}

/// Pull `(card.profile.device, Route)` out of a device's Route param.
pub fn parse_route(pod: &spa::pod::Pod) -> Option<(i32, Route)> {
    use spa::pod::deserialize::PodDeserializer;
    let (_, Value::Object(obj)) = PodDeserializer::deserialize_any_from(pod.as_bytes()).ok()? else { return None };
    let (mut index, mut device, mut channels) = (None, None, 0);
    for prop in obj.properties {
        match (prop.key, prop.value) {
            (spa::sys::SPA_PARAM_ROUTE_index, Value::Int(i)) => index = Some(i),
            (spa::sys::SPA_PARAM_ROUTE_device, Value::Int(d)) => device = Some(d),
            (spa::sys::SPA_PARAM_ROUTE_props, Value::Object(props)) => {
                for p in props.properties {
                    if let (spa::sys::SPA_PROP_channelVolumes, Value::ValueArray(ValueArray::Float(v))) = (p.key, p.value)
                    {
                        channels = v.len();
                    }
                }
            }
            _ => {}
        }
    }
    Some((device?, Route { index: index?, channels }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_param_round_trips() {
        let bytes = route_param(Route { index: 3, channels: 2 }, 7, Change::Volume(0.5)).unwrap();
        let pod = spa::pod::Pod::from_bytes(&bytes).unwrap();
        assert_eq!(parse_route(pod), Some((7, Route { index: 3, channels: 2 })));
    }

    #[test]
    fn volume_uses_the_cubic_scale() {
        use spa::pod::deserialize::PodDeserializer;
        let bytes = node_param(Change::Volume(0.5), 2).unwrap();
        let (_, Value::Object(obj)) = PodDeserializer::deserialize_any_from(&bytes).unwrap() else { panic!() };
        assert!(matches!(&obj.properties[0].value, Value::ValueArray(ValueArray::Float(v)) if v == &vec![0.125, 0.125]));
    }
}
