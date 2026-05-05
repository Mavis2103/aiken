use crate::server::Server;
use aiken_project::{config::ProjectConfig, paths};
use error::Error;
use lsp_server::Connection;
use std::env;

mod cast;
mod completion;
mod edits;
pub mod error;
mod quickfix;
mod rename;
mod semantic_tokens;
pub mod server;
mod signature_help;
pub mod utils;

#[allow(clippy::result_large_err)]
pub fn start() -> Result<(), Error> {
    tracing::info!("Aiken language server starting");

    // Forcibly disable colors on outputs for LSP
    owo_colors::set_override(false);

    let root = env::current_dir()?;

    let config = if paths::project_config().exists() {
        tracing::info!("Aiken project detected");

        Some(ProjectConfig::load(&root).expect("failed to load aiken.toml"))
    } else {
        tracing::info!("Aiken project config not found");

        None
    };

    // Create the transport. Includes the stdio (stdin and stdout) versions but this could
    // also be implemented to use sockets or HTTP.
    let (connection, io_threads) = Connection::stdio();

    // Run the server and wait for the two threads to end (typically by trigger LSP Exit event).
    let server_capabilities = serde_json::to_value(capabilities())?;

    let initialization_params = connection.initialize(server_capabilities)?;
    let initialize_params = serde_json::from_value(initialization_params)?;

    let mut server = Server::new(initialize_params, config, root);

    server.listen(connection)?;

    io_threads.join()?;

    tracing::info!("Aiken language server shutting down");

    Ok(())
}

fn capabilities() -> lsp_types::ServerCapabilities {
    lsp_types::ServerCapabilities {
        completion_provider: Some(lsp_types::CompletionOptions {
            resolve_provider: None,
            trigger_characters: Some(vec![".".into(), " ".into()]),
            all_commit_characters: None,
            completion_item: None,
            work_done_progress_options: lsp_types::WorkDoneProgressOptions {
                work_done_progress: None,
            },
        }),
        code_action_provider: Some(lsp_types::CodeActionProviderCapability::Simple(true)),
        document_formatting_provider: Some(lsp_types::OneOf::Left(true)),
        definition_provider: Some(lsp_types::OneOf::Left(true)),
        document_symbol_provider: Some(lsp_types::OneOf::Left(true)),
        references_provider: Some(lsp_types::OneOf::Left(true)),
        selection_range_provider: Some(lsp_types::SelectionRangeProviderCapability::Simple(true)),
        rename_provider: Some(lsp_types::OneOf::Right(lsp_types::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: lsp_types::WorkDoneProgressOptions {
                work_done_progress: None,
            },
        })),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        inlay_hint_provider: Some(lsp_types::OneOf::Left(true)),
        semantic_tokens_provider: Some(
            lsp_types::SemanticTokensServerCapabilities::SemanticTokensOptions(
                lsp_types::SemanticTokensOptions {
                    legend: semantic_tokens::semantic_tokens_legend(),
                    range: Some(true),
                    full: Some(lsp_types::SemanticTokensFullOptions::Bool(true)),
                    work_done_progress_options: lsp_types::WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                },
            ),
        ),
        workspace_symbol_provider: Some(lsp_types::OneOf::Left(true)),
        signature_help_provider: Some(lsp_types::SignatureHelpOptions {
            trigger_characters: Some(vec!["(".into(), ",".into()]),
            retrigger_characters: Some(vec![",".into()]),
            work_done_progress_options: lsp_types::WorkDoneProgressOptions {
                work_done_progress: None,
            },
        }),
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Options(
            lsp_types::TextDocumentSyncOptions {
                open_close: None,
                change: Some(lsp_types::TextDocumentSyncKind::FULL),
                will_save: Some(true),
                will_save_wait_until: Some(true),
                save: Some(lsp_types::TextDocumentSyncSaveOptions::SaveOptions(
                    lsp_types::SaveOptions {
                        include_text: Some(false),
                    },
                )),
            },
        )),
        ..Default::default()
    }
}
