use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct LpdfEngine {
    licensed: bool,
}

#[wasm_bindgen]
impl LpdfEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(license_key: &str) -> LpdfEngine {
        LpdfEngine {
            licensed: !license_key.is_empty(),
        }
    }

    pub fn render(&self, xml: &str) -> String {
        let watermark = if self.licensed {
            serde_json::json!(null)
        } else {
            serde_json::json!({
                "type": "lpdf:watermark",
                "text": "made with lpdf.io"
            })
        };

        serde_json::json!({
            "version": 1,
            "pages": [{ "width": 595, "height": 842, "nodes": [] }],
            "watermark": watermark,
            "input_length": xml.len()
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> serde_json::Value {
        serde_json::from_str(json).expect("render() returned invalid JSON")
    }

    #[test]
    fn unlicensed_render_includes_watermark() {
        let engine = LpdfEngine::new("");
        let result = parse(&engine.render("<doc/>"));
        assert!(
            !result["watermark"].is_null(),
            "watermark should be present for unlicensed engine"
        );
        assert_eq!(result["watermark"]["type"], "lpdf:watermark");
        assert_eq!(result["watermark"]["text"], "made with lpdf.io");
    }

    #[test]
    fn licensed_render_omits_watermark() {
        let engine = LpdfEngine::new("any-key");
        let result = parse(&engine.render("<doc/>"));
        assert!(
            result["watermark"].is_null(),
            "watermark should be absent for licensed engine"
        );
    }

    #[test]
    fn render_tree_has_expected_shape() {
        let engine = LpdfEngine::new("any-key");
        let result = parse(&engine.render("<doc/>"));
        assert_eq!(result["version"], 1);
        assert!(result["pages"].is_array());
        assert_eq!(result["pages"].as_array().unwrap().len(), 1);
        let page = &result["pages"][0];
        assert_eq!(page["width"], 595);
        assert_eq!(page["height"], 842);
    }

    #[test]
    fn render_records_input_length() {
        let engine = LpdfEngine::new("any-key");
        let xml = "<doc/>";
        let result = parse(&engine.render(xml));
        assert_eq!(result["input_length"], xml.len());
    }
}
