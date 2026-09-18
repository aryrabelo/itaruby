//! Performance baseline for the two phases that dominate an `ita check`
//! run: building the project index, and checking every file against it.
//!
//! The corpus here is SYNTHETIC on purpose. `testdata/` is the obvious
//! input and the wrong one: it grows every time a fixture lands, so a
//! ceiling measured against it would drift for reasons that have nothing
//! to do with the checker getting slower — the same class of mistake as
//! reading a stale bucket number, which this repository has already paid
//! for twice. The generator below is fixed, so a number moving means the
//! code moved.
//!
//! The shape is deliberately representative rather than minimal: cross-file
//! constant references, instance variables written in `initialize` and read
//! elsewhere, `is_a?` narrowing, and calls that resolve into another file's
//! class. A benchmark over `1 + 1` measures nothing this project cares
//! about.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use itaruby_semantic::{check_file, project_index, Db, ProjectFiles, SourceFile};

/// Files in the synthetic project. Fixed forever: this number is part of
/// the baseline's meaning, and changing it invalidates every recorded
/// measurement.
const FILES: usize = 40;

/// One synthetic Ruby file. `n` seeds every name so the files form a chain
/// — file `n` calls into file `n - 1` — which is what makes the index
/// actually get consulted instead of every lookup resolving locally.
fn source(n: usize) -> String {
    let prev = if n == 0 { FILES - 1 } else { n - 1 };
    format!(
        "class Widget{n}
  CODE{n} = \"w{n}\"

  def initialize
    @name = \"widget-{n}\"
    @count = {n}
  end

  def name
    @name
  end

  def label
    \"#{{@name}}/#{{@count}}\"
  end

  def neighbour
    Widget{prev}.new
  end

  def describe(other)
    if other.is_a?(Widget{prev})
      other.name
    else
      @name
    end
  end

  def chain
    neighbour.name
  end

  def constant_ref
    Widget{prev}::CODE{prev}
  end
end

class Helper{n}
  def run(w)
    w.label
  end
end
"
    )
}

fn build(db: &Db) -> Vec<SourceFile> {
    let files: Vec<SourceFile> = (0..FILES)
        .map(|n| SourceFile::new(db, format!("bench/widget_{n}.rb").into(), source(n)))
        .collect();
    ProjectFiles::new(db, files.clone());
    files
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("check");

    // Cold index build: a fresh salsa database every iteration, which is
    // what `ita check` does once per run.
    group.bench_function("project_index", |b| {
        b.iter_batched(
            || {
                let db = Db::default();
                build(&db);
                db
            },
            |db| {
                let index = project_index(&db);
                std::hint::black_box(index.classes.len())
            },
            BatchSize::SmallInput,
        );
    });

    // Index plus every diagnostic: the whole run, minus I/O and rendering.
    group.bench_function("index_and_check_all", |b| {
        b.iter_batched(
            || {
                let db = Db::default();
                let files = build(&db);
                (db, files)
            },
            |(db, files)| {
                let mut total = 0;
                for file in &files {
                    total += check_file(&db, *file).len();
                }
                std::hint::black_box(total)
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
