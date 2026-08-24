mod tui;

use siftx::field::{Record, TextOptions};
use siftx::index::Index;
use siftx::schema::{Schema, SchemaBuilder};
use siftx::{Document, FileDocumentSource, FileWalker, FsEntry};
use std::path::{Path, PathBuf};
use tui::tui_main;

fn main() {
    tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // tui_main().unwrap();
    let path = Path::new("/Users/eli/Documents/programming/rust/siftx/src/scratch");
    let dir = FsEntry::from_path(path.into());

    let schema = Schema::builder()
        .add_text_field(
            "title".to_string(),
            TextOptions {
                analyzer: "default".to_string(),
                record: Record::Basic,
                field_norms: true,
            },
            true,
            true,
        )
        .add_text_field(
            "body".to_string(),
            TextOptions {
                analyzer: "default".to_string(),
                record: Record::Basic,
                field_norms: true,
            },
            true,
            true,
        )
        .build();

    let index = Index::create_in_dir(path, schema.clone()).unwrap();
    let mut writer = index.writer().unwrap();
    let fsd = FileDocumentSource::new(path.join("books"), schema);
    for doc in fsd {
        let doc = doc.unwrap();
        println!("Iterating doc: {:?}", doc.id());
        writer.add_document(doc).unwrap()
    }
    writer.commit().unwrap();
}
