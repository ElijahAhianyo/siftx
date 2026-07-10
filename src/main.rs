mod tui;

use std::path::PathBuf;
use tui::tui_main;
use siftx::{FsEntry, FileWalker, Document, FileDocumentSource};
use siftx::field::{Record, TextOptions};
use siftx::schema::{Schema, SchemaBuilder};

fn main() {
    // tui_main().unwrap();
    let path = "/Users/eli/Documents/programming/rust/siftx/src/scratch";
    let dir = FsEntry::from_path(path.into());

    let schema = Schema::builder()
        .add_text_field(
            "title".to_string(),
            TextOptions {
                analyzer: "simple".to_string(),
                record: Record::Basic,
                field_norms: true,
            },
            true,
            true
        )
        .add_text_field(
        "body".to_string(),
        TextOptions {
            analyzer: "simple".to_string(),
            record: Record::Basic,
            field_norms: true,
        },
        true,
        true,
    )
        .build();

    let fsd = FileDocumentSource::new(path.into(), schema);
    for doc in fsd {
        println!("{:?}", doc.unwrap());
    }

    // let walker = FileWalker::new(dir);
    // for doc in walker {
    //     if let Ok(doc) = doc{
    //         let content_reader = doc.open().unwrap();
    //         for chunk in content_reader{
    //             println!("{:?}", chunk.unwrap());
    //         }
    //     }
    // }
}

