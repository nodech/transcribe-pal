use std::{fmt::Write, io::IsTerminal, path::PathBuf};

use anyhow::Context;
use clap::{Args, Subcommand};
use indicatif::{
    MultiProgress, ProgressBar, ProgressDrawTarget, ProgressState,
    ProgressStyle,
};
use tracing::{debug, instrument};

use crate::{
    format::{SizeBase, format_disk_size, print_format_table},
    model::{self, DownloadProgress, StoreDirectoryPath},
    transcribe::ModelKind,
};

#[derive(Debug, Args)]
pub(crate) struct ModelCommandArgs {
    #[command(subcommand)]
    model_command: ModelCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ModelCommand {
    /// List models
    List(ListCommandArgs),
    /// Download model
    Download(DownloadCommandArgs),
    // /// Verify model files
    // Verify(VerifyCommandArgs),
}

#[derive(Debug, Args)]
pub(crate) struct VerifyCommandArgs {
    model_name: String,
}

#[derive(Debug, Args)]
pub(crate) struct ListCommandArgs {}

#[derive(Debug, Args)]
pub(crate) struct DownloadCommandArgs {
    /// Model name
    model_name: String,

    /// Model store directory
    #[arg(long)]
    store_dir: Option<PathBuf>,

    /// Accept the license of the model.
    #[arg(long, short)]
    yes: bool,

    /// Overwrite files if necessary.
    #[arg(long, short)]
    force: bool,
}

pub(crate) fn run(cmd_args: ModelCommandArgs) -> anyhow::Result<()> {
    match cmd_args.model_command {
        ModelCommand::List(args) => list_models(args)?,
        ModelCommand::Download(args) => download_model(args)?,
    }

    Ok(())
}

fn list_models(_args: ListCommandArgs) -> anyhow::Result<()> {
    let model_manifests = model::load_manifests()?;

    let headers = vec![
        "model".to_string(),
        "size on disk".to_string(),
        "license".to_string(),
        "homepage".to_string(),
    ];

    let mut table = vec![headers];

    // TODO: List installed thingies for filtering.

    table.extend(model_manifests.into_values().map(|f| {
        let size_on_disk = f.size_on_disk();
        let (size, unit) =
            format_disk_size(size_on_disk as f64, SizeBase::Base2);

        vec![
            f.name.to_string(),
            format!("{size} {unit}"),
            f.license_name,
            f.homepage_url.to_string(),
        ]
    }));

    print_format_table(&table, 2);
    Ok(())
}

const NAME_WIDTH: usize = 44;
const BAR_WIDTH: usize = 42;

#[instrument(level = "debug", name = "download_model", skip_all)]
fn download_model(args: DownloadCommandArgs) -> anyhow::Result<()> {
    let store_dir = StoreDirectoryPath::from_opt_path(args.store_dir)?;
    let model: ModelKind = args.model_name.parse()?;

    let model_manifests = model::load_manifests()?;
    let manifest = model_manifests
        .get(model.to_name())
        .ok_or(anyhow::anyhow!("Could not find model: {}", model))?;

    let backend = model::FSBackend::new();
    let mut store = model::Store::new(store_dir, manifest, backend);

    store.ensure_dir().with_context(|| {
        format!("Failed to create \"{}\"", store.model_path().display())
    })?;

    let _guard = store.acquire_lock()?;

    let request = model::DownloadRequest::new(&mut store)?;

    debug!("download request created");

    let mut downloader = model::Download::new(&mut store, request);

    debug!("starting download");

    let all = multi();
    let active_style = pacman_style("cyan/blue", true)?;
    let fetch_style = pacman_style("yellow/blue", true)?;
    let verify_style = pacman_style("yellow/yellow", false)?;
    let done_style = pacman_style("green/green", false)?;

    let all_files = downloader.len();
    let total = all.add(ProgressBar::new(downloader.total_size()));
    total.set_style(active_style.clone());
    total.set_position(downloader.downloaded_size());
    total.set_prefix(format!("Total (0/{})", all_files));

    let mut done = 0;
    while let Some(mut dfile) = downloader.next() {
        let file_name = dfile.file_path().to_string_lossy().into_owned();
        let current = all.insert_before(
            &total,
            ProgressBar::new(dfile.expected_size().unwrap_or_default()),
        );

        current.set_position(dfile.downloaded_size().unwrap_or_default());
        current.set_style(fetch_style.clone());
        current.set_prefix(file_name);

        current.reset_elapsed();
        current.reset_eta();
        total.reset_elapsed();
        total.reset_eta();

        loop {
            let res = dfile.process()?;
            match res {
                DownloadProgress::Fetch => current.set_message("fetching..."),
                DownloadProgress::Resume {
                    downloaded,
                    downloaded_total,
                } => {
                    current.set_style(fetch_style.clone());
                    current.set_message("resumming...");
                    current.set_position(downloaded);

                    total.set_position(downloaded_total);

                    current.reset_elapsed();
                    current.reset_eta();
                    total.reset_elapsed();
                    total.reset_eta();
                }
                DownloadProgress::Progress {
                    downloaded,
                    downloaded_total,
                } => {
                    current.set_style(active_style.clone());
                    current.set_message("");
                    current.set_position(downloaded);

                    total.set_position(downloaded_total);
                }
                DownloadProgress::Verify => {
                    current.set_style(verify_style.clone());
                    current.set_message("verifying...");
                    current.force_draw();
                }
                DownloadProgress::Finalize => {
                    current.set_style(done_style.clone());
                    current.set_message("finalizing...");
                }
                DownloadProgress::Done => {
                    current.finish_with_message("");
                    done += 1;
                    total.set_prefix(format!("Total ({}/{})", done, all_files));
                    break;
                }
            }
        }
    }

    total.finish();

    Ok(())
}

fn progress_enabled() -> bool {
    let is_tty = std::io::stderr().is_terminal();
    let verbose_logs = tracing::enabled!(tracing::Level::DEBUG);

    is_tty && !verbose_logs
}

fn draw_target() -> ProgressDrawTarget {
    if progress_enabled() {
        ProgressDrawTarget::stderr()
    } else {
        ProgressDrawTarget::hidden()
    }
}

fn multi() -> MultiProgress {
    MultiProgress::with_draw_target(draw_target())
}

fn pacman_template(bar_style: &str, show_transfer: bool) -> String {
    let transfer = if show_transfer {
        "{size} {speed} {eta:>5}"
    } else {
        "{size} {blank_speed} {blank_eta}"
    };

    format!(
        " {{prefix:<{NAME_WIDTH}}} \
           {transfer} \
           [{{bar:{BAR_WIDTH}.{bar_style}}}] \
           {{percent:>3}}% {{msg}}",
    )
}

fn pacman_style(
    bar_style: &str,
    show_transfer: bool,
) -> anyhow::Result<ProgressStyle> {
    Ok(ProgressStyle::with_template(&pacman_template(
        bar_style,
        show_transfer,
    ))?
    .with_key("size", write_size)
    .with_key("speed", write_speed)
    .with_key("blank_speed", write_blank_speed)
    .with_key("blank_eta", write_blank_eta)
    .progress_chars("#>-"))
}

fn write_size(state: &ProgressState, w: &mut dyn Write) {
    let _ = write!(w, "{}", size_column(state.pos() as f64));
}

fn write_speed(state: &ProgressState, w: &mut dyn Write) {
    let _ = write!(w, "{}", speed_column(state.per_sec()));
}

fn write_blank_speed(_state: &ProgressState, w: &mut dyn Write) {
    let _ = write!(w, "{:>14}", "");
}

fn write_blank_eta(_state: &ProgressState, w: &mut dyn Write) {
    let _ = write!(w, "{:>5}", "");
}

fn size_column(bytes: f64) -> String {
    let (value, unit) = format_disk_size(bytes, SizeBase::Base2);
    format!("{value:>8} {unit:<3}")
}

fn speed_column(bytes_per_sec: f64) -> String {
    let (value, unit) = format_disk_size(bytes_per_sec, SizeBase::Base2);
    format!("{value:>8} {unit:<3}/s")
}
