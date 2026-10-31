//! Color spaces selected by `cs`/`CS`: their family and initial color.

use hayro_syntax::object::{Name, Object};
use hayro_syntax::page::Resources;

use super::types::ColorInfo;

/// The initial color of a space of `family` with `n` components (ISO 32000 8.6).
fn initial_color(family: &str, n: usize) -> Vec<f64> {
    match family {
        "DeviceGray" | "CalGray" | "Indexed" | "I" => vec![0.0],
        "DeviceRGB" | "CalRGB" | "Lab" => vec![0.0; 3],
        "DeviceCMYK" => vec![0.0, 0.0, 0.0, 1.0],
        "Separation" => vec![1.0],
        "DeviceN" => vec![1.0; n],
        "ICCBased" if n == 4 => vec![0.0, 0.0, 0.0, 1.0],
        "ICCBased" => vec![0.0; n.max(1)],
        _ => Vec::new(),
    }
}

/// Family, component count hint and initial color for a color-space resource or device name.
pub(super) fn color_space_info(resources: &Resources<'_>, name: &Name<'_>) -> ColorInfo {
    let device = |family: &str| ColorInfo {
        space: family.to_owned(),
        resource: None,
        components: initial_color(family, 0),
        pattern: None,
    };
    match name.as_str() {
        "DeviceGray" | "G" => return device("DeviceGray"),
        "DeviceRGB" | "RGB" => return device("DeviceRGB"),
        "DeviceCMYK" | "CMYK" => return device("DeviceCMYK"),
        "Pattern" => return device("Pattern"),
        _ => {}
    }
    let (space, n) = match resources.get_color_space(name) {
        Some(Object::Name(n)) => (n.as_str().to_owned(), 0),
        Some(Object::Array(arr)) => {
            let mut it = arr.iter::<Object<'_>>();
            let family = match it.next() {
                Some(Object::Name(f)) => f.as_str().to_owned(),
                _ => String::from("Unknown"),
            };
            let second = it.next();
            let n = match (&*family, second) {
                ("ICCBased", Some(Object::Stream(s))) => s.dict().get::<usize>(b"N").unwrap_or(3),
                ("DeviceN", Some(Object::Array(names))) => names.raw_iter().count(),
                _ => 0,
            };
            (family, n)
        }
        _ => (String::from("Unknown"), 0),
    };
    ColorInfo {
        components: initial_color(&space, n),
        space,
        resource: Some(name.as_str().to_owned()),
        pattern: None,
    }
}

/// The space and stroke flag of a device color operator (`g G rg RG k K`).
pub(super) fn device_operator(operator: &str) -> (&'static str, bool) {
    match operator {
        "g" => ("DeviceGray", false),
        "G" => ("DeviceGray", true),
        "rg" => ("DeviceRGB", false),
        "RG" => ("DeviceRGB", true),
        "k" => ("DeviceCMYK", false),
        _ => ("DeviceCMYK", true),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn initial_colors_follow_the_family() {
        let colors: Vec<Vec<f64>> = [
            ("DeviceGray", 0),
            ("DeviceCMYK", 0),
            ("DeviceN", 2),
            ("ICCBased", 4),
            ("ICCBased", 0),
            ("Pattern", 0),
        ]
        .iter()
        .map(|(family, n)| initial_color(family, *n))
        .collect();

        assert_eq!(
            colors,
            [
                vec![0.0],
                vec![0.0, 0.0, 0.0, 1.0],
                vec![1.0, 1.0],
                vec![0.0, 0.0, 0.0, 1.0],
                vec![0.0],
                vec![],
            ]
        );
        // smoke: device operators pick their space and side
        assert_eq!(device_operator("RG"), ("DeviceRGB", true));
    }
}
