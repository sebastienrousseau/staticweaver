// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![allow(missing_docs)]

//! Comparative benchmarks: staticweaver vs Tera, Minijinja, Askama.
//!
//! Each workload is implemented in all four engines using each engine's
//! own template syntax, then registered as a Criterion *group* so the
//! HTML report puts the four bars next to each other. Run with:
//!
//! ```text
//! cargo bench --bench comparative
//! ```
//!
//! Templates are equivalent in *output*, not literal text — Mustache,
//! Jinja, and Askama have different syntax. We bench what each engine
//! is asked to produce, not how the input is spelled.

use askama::Template;
use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion,
};
use minijinja::Environment;
use staticweaver::{Context as SwContext, Engine as SwEngine};
use std::time::Duration;
use tera::{Context as TeraContext, Tera};

// ─── Workload 1: simple substitution ─────────────────────────────────

const SW_SIMPLE: &str = "Hello, {{name}}!";
const TERA_SIMPLE: &str = "Hello, {{name}}!";
const MJ_SIMPLE: &str = "Hello, {{ name }}!";

#[derive(Template)]
#[template(source = "Hello, {{ name }}!", ext = "txt")]
struct AskamaSimple<'a> {
    name: &'a str,
}

fn bench_simple(c: &mut Criterion) {
    let mut group = c.benchmark_group("simple_sub");

    // staticweaver
    let sw = SwEngine::new("", Duration::from_secs(60));
    let mut sw_ctx = SwContext::new();
    sw_ctx.set("name".to_string(), "Alice".to_string());
    let _ = group.bench_function(
        BenchmarkId::new("staticweaver", ""),
        |b| {
            b.iter(|| {
                let _ = black_box(
                    sw.render_template(
                        black_box(SW_SIMPLE),
                        black_box(&sw_ctx),
                    )
                    .unwrap(),
                );
            });
        },
    );

    // tera (precompiled)
    let mut tera = Tera::default();
    tera.add_raw_template("simple", TERA_SIMPLE).unwrap();
    let mut tctx = TeraContext::new();
    tctx.insert("name", "Alice");
    let _ = group.bench_function(BenchmarkId::new("tera", ""), |b| {
        b.iter(|| {
            let _ = black_box(
                tera.render(black_box("simple"), black_box(&tctx))
                    .unwrap(),
            );
        });
    });

    // minijinja (precompiled)
    let mut env = Environment::new();
    env.add_template("simple", MJ_SIMPLE).unwrap();
    let tmpl = env.get_template("simple").unwrap();
    let _ =
        group.bench_function(BenchmarkId::new("minijinja", ""), |b| {
            b.iter(|| {
                let _ = black_box(
                    tmpl.render(black_box(
                        minijinja::context! { name => "Alice" },
                    ))
                    .unwrap(),
                );
            });
        });

    // askama (compile-time)
    let _ = group.bench_function(BenchmarkId::new("askama", ""), |b| {
        b.iter(|| {
            let _ = black_box(
                AskamaSimple {
                    name: black_box("Alice"),
                }
                .render()
                .unwrap(),
            );
        });
    });

    group.finish();
}

// ─── Workload 2: many substitutions (32 tags) ────────────────────────

fn build_sw_many() -> String {
    let mut s = String::new();
    for i in 0..32 {
        s.push_str(&format!("{{{{k{i}}}}} "));
    }
    s
}

fn build_tera_many() -> String {
    let mut s = String::new();
    for i in 0..32 {
        s.push_str(&format!("{{{{ k{i} }}}} "));
    }
    s
}

#[derive(Template)]
#[template(
    source = "\
{{ k0 }} {{ k1 }} {{ k2 }} {{ k3 }} {{ k4 }} {{ k5 }} {{ k6 }} {{ k7 }} \
{{ k8 }} {{ k9 }} {{ k10 }} {{ k11 }} {{ k12 }} {{ k13 }} {{ k14 }} {{ k15 }} \
{{ k16 }} {{ k17 }} {{ k18 }} {{ k19 }} {{ k20 }} {{ k21 }} {{ k22 }} {{ k23 }} \
{{ k24 }} {{ k25 }} {{ k26 }} {{ k27 }} {{ k28 }} {{ k29 }} {{ k30 }} {{ k31 }}\
",
    ext = "txt"
)]
struct AskamaMany {
    k0: String,
    k1: String,
    k2: String,
    k3: String,
    k4: String,
    k5: String,
    k6: String,
    k7: String,
    k8: String,
    k9: String,
    k10: String,
    k11: String,
    k12: String,
    k13: String,
    k14: String,
    k15: String,
    k16: String,
    k17: String,
    k18: String,
    k19: String,
    k20: String,
    k21: String,
    k22: String,
    k23: String,
    k24: String,
    k25: String,
    k26: String,
    k27: String,
    k28: String,
    k29: String,
    k30: String,
    k31: String,
}

fn bench_many(c: &mut Criterion) {
    let mut group = c.benchmark_group("many_sub_32");

    let sw_tmpl = build_sw_many();
    let tera_tmpl = build_tera_many();

    // staticweaver
    let sw = SwEngine::new("", Duration::from_secs(60));
    let mut sw_ctx = SwContext::new();
    for i in 0..32 {
        sw_ctx.set(format!("k{i}"), format!("v{i}"));
    }
    let _ = group.bench_function(
        BenchmarkId::new("staticweaver", ""),
        |b| {
            b.iter(|| {
                let _ = black_box(
                    sw.render_template(
                        black_box(&sw_tmpl),
                        black_box(&sw_ctx),
                    )
                    .unwrap(),
                );
            });
        },
    );

    // tera
    let mut tera = Tera::default();
    tera.add_raw_template("many", &tera_tmpl).unwrap();
    let mut tctx = TeraContext::new();
    for i in 0..32 {
        tctx.insert(format!("k{i}"), &format!("v{i}"));
    }
    let _ = group.bench_function(BenchmarkId::new("tera", ""), |b| {
        b.iter(|| {
            let _ = black_box(
                tera.render(black_box("many"), black_box(&tctx))
                    .unwrap(),
            );
        });
    });

    // minijinja (same Jinja syntax as tera)
    let mut env = Environment::new();
    env.add_template_owned("many".to_string(), tera_tmpl.clone())
        .unwrap();
    let tmpl = env.get_template("many").unwrap();
    let mj_ctx: std::collections::HashMap<String, String> = (0..32)
        .map(|i| (format!("k{i}"), format!("v{i}")))
        .collect();
    let _ =
        group.bench_function(BenchmarkId::new("minijinja", ""), |b| {
            b.iter(|| {
                let _ =
                    black_box(tmpl.render(black_box(&mj_ctx)).unwrap());
            });
        });

    // askama
    let askama = AskamaMany {
        k0: "v0".into(),
        k1: "v1".into(),
        k2: "v2".into(),
        k3: "v3".into(),
        k4: "v4".into(),
        k5: "v5".into(),
        k6: "v6".into(),
        k7: "v7".into(),
        k8: "v8".into(),
        k9: "v9".into(),
        k10: "v10".into(),
        k11: "v11".into(),
        k12: "v12".into(),
        k13: "v13".into(),
        k14: "v14".into(),
        k15: "v15".into(),
        k16: "v16".into(),
        k17: "v17".into(),
        k18: "v18".into(),
        k19: "v19".into(),
        k20: "v20".into(),
        k21: "v21".into(),
        k22: "v22".into(),
        k23: "v23".into(),
        k24: "v24".into(),
        k25: "v25".into(),
        k26: "v26".into(),
        k27: "v27".into(),
        k28: "v28".into(),
        k29: "v29".into(),
        k30: "v30".into(),
        k31: "v31".into(),
    };
    let _ = group.bench_function(BenchmarkId::new("askama", ""), |b| {
        b.iter(|| {
            let _ = black_box(askama.render().unwrap());
        });
    });

    group.finish();
}

// ─── Workload 3: HTML escape heavy ───────────────────────────────────

fn metachar_body() -> String {
    (0..10_000)
        .map(|i| match i % 20 {
            0 => '<',
            1 => '>',
            2 => '&',
            _ => 'x',
        })
        .collect()
}

#[derive(Template)]
#[template(source = "<div>{{ body }}</div>", ext = "html")]
struct AskamaEscape<'a> {
    body: &'a str,
}

fn bench_escape(c: &mut Criterion) {
    let mut group = c.benchmark_group("escape_heavy");
    let body = metachar_body();

    // staticweaver
    let sw = SwEngine::new("", Duration::from_secs(60));
    let mut sw_ctx = SwContext::new();
    sw_ctx.set("body".to_string(), body.clone());
    let _ = group.bench_function(
        BenchmarkId::new("staticweaver", ""),
        |b| {
            b.iter(|| {
                let _ = black_box(
                    sw.render_template(
                        black_box("<div>{{body}}</div>"),
                        black_box(&sw_ctx),
                    )
                    .unwrap(),
                );
            });
        },
    );

    // tera (autoescape requires .html extension)
    let mut tera = Tera::default();
    tera.add_raw_template("escape.html", "<div>{{ body }}</div>")
        .unwrap();
    tera.autoescape_on(vec![".html"]);
    let mut tctx = TeraContext::new();
    tctx.insert("body", &body);
    let _ = group.bench_function(BenchmarkId::new("tera", ""), |b| {
        b.iter(|| {
            let _ = black_box(
                tera.render(black_box("escape.html"), black_box(&tctx))
                    .unwrap(),
            );
        });
    });

    // minijinja with autoescape
    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::Html);
    env.add_template("escape", "<div>{{ body }}</div>").unwrap();
    let tmpl = env.get_template("escape").unwrap();
    let _ =
        group.bench_function(BenchmarkId::new("minijinja", ""), |b| {
            b.iter(|| {
                let _ = black_box(
                    tmpl.render(black_box(
                        minijinja::context! { body => &body },
                    ))
                    .unwrap(),
                );
            });
        });

    // askama (autoescapes with .html extension)
    let _ = group.bench_function(BenchmarkId::new("askama", ""), |b| {
        b.iter(|| {
            let _ = black_box(
                AskamaEscape {
                    body: black_box(&body),
                }
                .render()
                .unwrap(),
            );
        });
    });

    group.finish();
}

// ─── Workload 4: each loop, 100 items ────────────────────────────────

const SW_EACH: &str =
    "<ul>{{#each items}}<li>{{this}}</li>{{/each}}</ul>";
const JINJA_EACH: &str =
    "<ul>{% for item in items %}<li>{{ item }}</li>{% endfor %}</ul>";

#[derive(Template)]
#[template(
    source = "<ul>{% for item in items %}<li>{{ item }}</li>{% endfor %}</ul>",
    ext = "html"
)]
struct AskamaEach<'a> {
    items: &'a [String],
}

fn bench_each_100(c: &mut Criterion) {
    let mut group = c.benchmark_group("each_100");

    let items: Vec<String> =
        (0..100).map(|i| format!("item-{i}")).collect();

    // staticweaver
    let sw = SwEngine::new("", Duration::from_secs(60));
    let mut sw_ctx = SwContext::new();
    sw_ctx.set_value("items".to_string(), items.clone());
    let _ = group.bench_function(
        BenchmarkId::new("staticweaver", ""),
        |b| {
            b.iter(|| {
                let _ = black_box(
                    sw.render_template(
                        black_box(SW_EACH),
                        black_box(&sw_ctx),
                    )
                    .unwrap(),
                );
            });
        },
    );

    // tera
    let mut tera = Tera::default();
    tera.add_raw_template("each", JINJA_EACH).unwrap();
    let mut tctx = TeraContext::new();
    tctx.insert("items", &items);
    let _ = group.bench_function(BenchmarkId::new("tera", ""), |b| {
        b.iter(|| {
            let _ = black_box(
                tera.render(black_box("each"), black_box(&tctx))
                    .unwrap(),
            );
        });
    });

    // minijinja
    let mut env = Environment::new();
    env.add_template("each", JINJA_EACH).unwrap();
    let tmpl = env.get_template("each").unwrap();
    let _ =
        group.bench_function(BenchmarkId::new("minijinja", ""), |b| {
            b.iter(|| {
                let _ = black_box(
                    tmpl.render(black_box(
                        minijinja::context! { items => &items },
                    ))
                    .unwrap(),
                );
            });
        });

    // askama
    let askama = AskamaEach { items: &items };
    let _ = group.bench_function(BenchmarkId::new("askama", ""), |b| {
        b.iter(|| {
            let _ = black_box(askama.render().unwrap());
        });
    });

    group.finish();
}

// ─── Workload 5: each loop, 1000 items (the big one) ─────────────────

fn bench_each_1000(c: &mut Criterion) {
    let mut group = c.benchmark_group("each_1000");
    let _ = group.sample_size(40); // fewer samples; this one is slow

    let items: Vec<String> =
        (0..1000).map(|i| format!("item-{i}")).collect();

    let sw = SwEngine::new("", Duration::from_secs(60));
    let mut sw_ctx = SwContext::new();
    sw_ctx.set_value("items".to_string(), items.clone());
    let _ = group.bench_function(
        BenchmarkId::new("staticweaver", ""),
        |b| {
            b.iter(|| {
                let _ = black_box(
                    sw.render_template(
                        black_box(SW_EACH),
                        black_box(&sw_ctx),
                    )
                    .unwrap(),
                );
            });
        },
    );

    let mut tera = Tera::default();
    tera.add_raw_template("each", JINJA_EACH).unwrap();
    let mut tctx = TeraContext::new();
    tctx.insert("items", &items);
    let _ = group.bench_function(BenchmarkId::new("tera", ""), |b| {
        b.iter(|| {
            let _ = black_box(
                tera.render(black_box("each"), black_box(&tctx))
                    .unwrap(),
            );
        });
    });

    let mut env = Environment::new();
    env.add_template("each", JINJA_EACH).unwrap();
    let tmpl = env.get_template("each").unwrap();
    let _ =
        group.bench_function(BenchmarkId::new("minijinja", ""), |b| {
            b.iter(|| {
                let _ = black_box(
                    tmpl.render(black_box(
                        minijinja::context! { items => &items },
                    ))
                    .unwrap(),
                );
            });
        });

    let askama = AskamaEach { items: &items };
    let _ = group.bench_function(BenchmarkId::new("askama", ""), |b| {
        b.iter(|| {
            let _ = black_box(askama.render().unwrap());
        });
    });

    group.finish();
}

// ─── Workload 6: if / else branching ─────────────────────────────────

const SW_IF: &str = "{{#if active}}ACTIVE{{else}}{{#if pending}}PENDING{{else}}OTHER{{/if}}{{/if}}";
const JINJA_IF: &str = "{% if active %}ACTIVE{% else %}{% if pending %}PENDING{% else %}OTHER{% endif %}{% endif %}";

#[derive(Template)]
#[template(
    source = "{% if active %}ACTIVE{% else %}{% if pending %}PENDING{% else %}OTHER{% endif %}{% endif %}",
    ext = "txt"
)]
struct AskamaIf {
    active: bool,
    pending: bool,
}

fn bench_if(c: &mut Criterion) {
    let mut group = c.benchmark_group("if_chain");

    let sw = SwEngine::new("", Duration::from_secs(60));
    let mut sw_ctx = SwContext::new();
    sw_ctx.set_value("active".to_string(), false);
    sw_ctx.set_value("pending".to_string(), true);
    let _ = group.bench_function(
        BenchmarkId::new("staticweaver", ""),
        |b| {
            b.iter(|| {
                let _ = black_box(
                    sw.render_template(
                        black_box(SW_IF),
                        black_box(&sw_ctx),
                    )
                    .unwrap(),
                );
            });
        },
    );

    let mut tera = Tera::default();
    tera.add_raw_template("if", JINJA_IF).unwrap();
    let mut tctx = TeraContext::new();
    tctx.insert("active", &false);
    tctx.insert("pending", &true);
    let _ = group.bench_function(BenchmarkId::new("tera", ""), |b| {
        b.iter(|| {
            let _ = black_box(
                tera.render(black_box("if"), black_box(&tctx)).unwrap(),
            );
        });
    });

    let mut env = Environment::new();
    env.add_template("if", JINJA_IF).unwrap();
    let tmpl = env.get_template("if").unwrap();
    let _ =
        group.bench_function(BenchmarkId::new("minijinja", ""), |b| {
            b.iter(|| {
                let _ = black_box(
                    tmpl.render(black_box(minijinja::context! {
                        active => false,
                        pending => true,
                    }))
                    .unwrap(),
                );
            });
        });

    let askama = AskamaIf {
        active: false,
        pending: true,
    };
    let _ = group.bench_function(BenchmarkId::new("askama", ""), |b| {
        b.iter(|| {
            let _ = black_box(askama.render().unwrap());
        });
    });

    group.finish();
}

// ─── Workload 7: filter pipeline ─────────────────────────────────────

const SW_FILTER: &str = "{{ name | trim | uppercase }}";
const JINJA_FILTER: &str = "{{ name | trim | upper }}";

#[derive(Template)]
#[template(source = "{{ name|trim|upper }}", ext = "txt")]
struct AskamaFilter<'a> {
    name: &'a str,
}

fn bench_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("filter_chain");

    let sw = SwEngine::new("", Duration::from_secs(60));
    let mut sw_ctx = SwContext::new();
    sw_ctx.set("name".to_string(), "  hello world  ".to_string());
    let _ = group.bench_function(
        BenchmarkId::new("staticweaver", ""),
        |b| {
            b.iter(|| {
                let _ = black_box(
                    sw.render_template(
                        black_box(SW_FILTER),
                        black_box(&sw_ctx),
                    )
                    .unwrap(),
                );
            });
        },
    );

    let mut tera = Tera::default();
    tera.add_raw_template("filter", JINJA_FILTER).unwrap();
    let mut tctx = TeraContext::new();
    tctx.insert("name", "  hello world  ");
    let _ = group.bench_function(BenchmarkId::new("tera", ""), |b| {
        b.iter(|| {
            let _ = black_box(
                tera.render(black_box("filter"), black_box(&tctx))
                    .unwrap(),
            );
        });
    });

    let mut env = Environment::new();
    env.add_template("filter", JINJA_FILTER).unwrap();
    let tmpl = env.get_template("filter").unwrap();
    let _ = group.bench_function(BenchmarkId::new("minijinja", ""), |b| {
        b.iter(|| {
            let _ = black_box(
                tmpl.render(black_box(minijinja::context! { name => "  hello world  " }))
                    .unwrap(),
            );
        });
    });

    // askama: trim and upper as filters need explicit pipe syntax
    let _ = group.bench_function(BenchmarkId::new("askama", ""), |b| {
        b.iter(|| {
            let _ = black_box(
                AskamaFilter {
                    name: black_box("  hello world  "),
                }
                .render()
                .unwrap(),
            );
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_simple,
    bench_many,
    bench_escape,
    bench_each_100,
    bench_each_1000,
    bench_if,
    bench_filter,
);
criterion_main!(benches);
