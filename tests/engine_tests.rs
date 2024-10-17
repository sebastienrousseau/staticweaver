// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Unit tests for the `Engine` struct and its methods.

#[cfg(test)]
mod tests {
    use fnv::FnvHashMap;
    use staticweaver::engine::EngineError;
    use staticweaver::{Context, Engine, PageOptions};
    use std::fs::File;
    use std::io::Write;
    use std::time::Duration;
    use tempfile::tempdir;

    /// Helper function to create an `Engine` instance.
    fn create_engine() -> Engine {
        Engine::new("dummy/path", Duration::from_secs(60))
    }

    /// Helper function to create a basic context with default values.
    fn create_basic_context() -> FnvHashMap<String, String> {
        let mut context = FnvHashMap::default();
        let _ = context.insert("name".to_string(), "World".to_string());
        let _ = context.insert("greeting".to_string(), "Hello".to_string());
        context
    }

    /// Helper function to assert template rendering results.
    fn assert_template_rendering(
        engine: &Engine,
        template: &str,
        context: &FnvHashMap<String, String>,
        expected_result: Result<&str, EngineError>,
    ) {
        let context: std::collections::HashMap<String, String> = context.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let result = engine.render_template(template, &context);
        match expected_result {
            Ok(expected) => assert_eq!(result.unwrap(), expected),
            Err(_) => assert!(result.is_err()),
        }
    }

    /// Tests for template rendering in the `Engine` struct.
    mod render_tests {
        use super::*;

        #[test]
        fn test_engine_render_template() {
            let engine = create_engine();
            let context = create_basic_context();
            let template = "{{greeting}}, {{name}}!";
            assert_template_rendering(
                &engine,
                template,
                &context,
                Ok("Hello, World!"),
            );
        }

        #[test]
        fn test_engine_render_template_unresolved_tags() {
            let engine = create_engine();
            let context = FnvHashMap::default();
            let template = "{{greeting}}, {{name}}!";
            assert_template_rendering(
                &engine,
                template,
                &context,
                Err(EngineError::Render(
                    "Unresolved template tag: greeting".to_string(),
                )),
            );
        }

        #[test]
        fn test_engine_render_empty_template() {
            let engine = create_engine();
            let context = FnvHashMap::default();
            let template = "";
            assert_template_rendering(
                &engine,
                template,
                &context,
                Err(EngineError::InvalidTemplate(
                    "Template is empty".to_string(),
                )),
            );
        }

        #[test]
        fn test_engine_render_special_characters_in_context() {
            let engine = create_engine();
            let mut context = FnvHashMap::default();
            let _ = context.insert(
                "name".to_string(),
                "<script>alert('XSS')</script>".to_string(),
            );
            let _ = context.insert("greeting".to_string(), "&".to_string());
            let template = "{{greeting}} {{name}}";
            assert_template_rendering(
                &engine,
                template,
                &context,
                Ok("& <script>alert('XSS')</script>"),
            );
        }

        #[test]
        fn test_engine_large_context() {
            let engine = create_engine();
            let mut context = FnvHashMap::default();
            let keys: Vec<String> =
                (0..1000).map(|i| format!("key{}", i)).collect();
            let values: Vec<String> =
                (0..1000).map(|i| format!("value{}", i)).collect();

            for i in 0..1000 {
                let _ = context.insert(keys[i].clone(), values[i].clone());
            }

            let mut template = String::new();
            for i in 0..1000 {
                template.push_str(&format!("{{{{key{}}}}}", i));
            }

            let context: std::collections::HashMap<_, _> = context.into_iter().collect();
            let result = engine.render_template(&template, &context).unwrap();
            let expected_result =
                (0..1000).fold(String::new(), |mut acc, i| {
                    use std::fmt::Write;
                    write!(&mut acc, "value{}", i).unwrap();
                    acc
                });

            assert_eq!(result, expected_result);
        }
    }

    /// Tests related to file operations, such as downloading templates.
    mod file_tests {
        use super::*;
        use staticweaver::engine::EngineError::Io;

        #[test]
        fn test_engine_download_file() {
            let engine = create_engine();
            let url = "https://raw.githubusercontent.com/sebastienrousseau/shokunin/main/template";
            let result = engine.create_template_folder(Some(url));
            assert!(result.is_ok());
        }

        #[test]
        fn test_engine_invalid_template_path() {
            let mut engine =
                Engine::new("invalid/path", Duration::from_secs(60));
            let context = Context {
                elements: FnvHashMap::default(),
            };
            let result =
                engine.render_page(&context, "nonexistent_layout");
            assert!(matches!(result, Err(Io(_))));
        }

        #[test]
        fn test_render_page_valid_path() {
            let temp_dir = tempdir().unwrap();
            let layout_path = temp_dir.path().join("layout.html");

            let mut file = File::create(&layout_path)
                .expect("Failed to create temp layout file");
            writeln!(file, "<html><body>{{{{greeting}}}}, {{{{name}}}}</body></html>")
                .expect("Failed to write content to layout file");

            println!(
                "Layout directory path: {}",
                temp_dir.path().to_str().unwrap()
            );

            let mut engine = Engine::new(
                temp_dir.path().to_str().unwrap(),
                Duration::from_secs(60),
            );

            let context = Context {
                elements: create_basic_context(),
            };

            let result = engine.render_page(&context, "layout");
            assert!(
                result.is_ok(),
                "Failed to render page, result: {:?}",
                result
            );

            let rendered_page = result.unwrap();
            println!("Rendered page: {}", rendered_page.trim());

            assert_eq!(
                rendered_page.trim(),
                "<html><body>Hello, World</body></html>"
            );
        }

        #[test]
        fn test_render_page_missing_file() {
            let mut engine =
                Engine::new("missing/path", Duration::from_secs(60));
            let context = Context {
                elements: FnvHashMap::default(),
            };
            let result =
                engine.render_page(&context, "nonexistent_layout");
            assert!(matches!(result, Err(Io(_))));
        }
    }

    /// Tests for the `PageOptions` struct.
    mod page_options_tests {
        use super::*;

        #[test]
        fn test_page_options_new() {
            let options = PageOptions::new();
            assert!(options.elements.is_empty());
        }

        #[test]
        fn test_page_options_set_get() {
            let mut options = PageOptions::new();
            options.set("title".to_string(), "My Title".to_string());
            assert_eq!(
                options.get("title"),
                Some(&"My Title".to_string())
            );
            assert_eq!(options.get("non_existent"), None);
        }

        #[test]
        fn test_page_options_large_context() {
            let mut options = PageOptions::new();
            for i in 0..1000 {
                let key = format!("key{}", i);
                let value = format!("value{}", i);
                options.set(key, value);
            }
            assert_eq!(
                options.get("key999"),
                Some(&"value999".to_string())
            );
            assert_eq!(options.get("key1000"), None);
        }
    }

    /// Edge case tests for template rendering.
    mod context_edge_cases_tests {
        use super::*;

        #[test]
        fn test_render_template_invalid_format() {
            let engine = create_engine();
            let context = create_basic_context();
            let template = "{greeting}, {name}!";
            assert_template_rendering(
                &engine,
                template,
                &context,
                Err(EngineError::InvalidTemplate("Invalid template format: single curly braces detected".to_string())),
            );
        }

        #[test]
        fn test_render_template_invalid_syntax() {
            let engine = create_engine();
            let context = create_basic_context();
            let invalid_template = "Hello, {{name";
            assert_template_rendering(
                &engine,
                invalid_template,
                &context,
                Err(EngineError::InvalidTemplate(
                    "Unclosed template tag".to_string(),
                )),
            );
        }

        #[test]
        fn test_render_large_template() {
            let engine = create_engine();
            let large_template = "Hello, {{name}}".repeat(1000);
            let context = create_basic_context();
            assert_template_rendering(
                &engine,
                &large_template,
                &context,
                Ok(&"Hello, World".repeat(1000)),
            );
        }

        #[test]
        fn test_render_template_empty_template() {
            let engine = create_engine();
            let context = create_basic_context();
            let empty_template = "";
            assert_template_rendering(
                &engine,
                empty_template,
                &context,
                Err(EngineError::InvalidTemplate(
                    "Template is empty".to_string(),
                )),
            );
        }

        #[test]
        fn test_clear_cache() {
            let mut engine =
                Engine::new("templates", Duration::from_secs(3600));

            let _ = engine
                .render_cache
                .insert("key1".to_string(), "value1".to_string());
            assert!(!engine.render_cache.is_empty());

            // Clear the cache
            engine.clear_cache();
            assert!(engine.render_cache.is_empty());
        }

        #[test]
        fn test_set_max_cache_size() {
            let mut engine =
                Engine::new("templates", Duration::from_secs(3600));

            // Insert multiple entries to simulate cache size exceeding max limit
            let _ = engine
                .render_cache
                .insert("key1".to_string(), "value1".to_string());
            let _ = engine
                .render_cache
                .insert("key2".to_string(), "value2".to_string());
            assert_eq!(engine.render_cache.len(), 2);

            // Set max cache size to 1
            engine.set_max_cache_size(1);
            // Cache should be cleared as the limit is exceeded
            assert!(engine.render_cache.is_empty());
        }
    }
}
