use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use siftx::DocumentId;
use siftx::field::FieldId;
use siftx::posting::{PostingsBuilder, Term};
use std::hint::black_box;

fn bench_record(c: &mut Criterion) {
    let mut group = c.benchmark_group("postings_record");

    for &n in &[100usize, 1_000, 10_000] {
        group.throughput(Throughput::Elements(n as u64));

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                PostingsBuilder::new,
                |mut builder| {
                    for doc_id in 0..n {
                        let term = Term::new(FieldId(0), black_box("example"));
                        builder.record(term, DocumentId(doc_id as u32), 0)
                    }
                    black_box(builder)
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_record);
criterion_main!(benches);
