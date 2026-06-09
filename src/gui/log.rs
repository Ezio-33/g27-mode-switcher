//! Journal de l'interface : tampon partagé, pont `tracing`, et rendu.
//!
//! Un [`LogBuffer`] (partagé via `Arc`) reçoit à la fois les lignes poussées par
//! l'application (résultats d'opérations, ✓ / •) et les événements `tracing`
//! captés par une couche dédiée. La zone de journal de la GUI l'affiche.

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use super::theme;

/// Nombre maximal de lignes conservées dans le journal.
const MAX_LINES: usize = 250;

/// Nature d'une ligne de journal (détermine préfixe et couleur).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Information neutre (pastille « • » or).
    Info,
    /// Opération réussie (pastille « • » verte).
    Success,
    /// Échec ou avertissement (pastille « • » rouge).
    Error,
}

/// Une ligne de journal.
#[derive(Debug, Clone)]
struct LogLine {
    kind: LineKind,
    text: String,
}

/// Tampon de journal partagé entre l'application, le pont `tracing` et la GUI.
#[derive(Clone)]
pub struct LogBuffer(Arc<Mutex<VecDeque<LogLine>>>);

impl LogBuffer {
    /// Crée un tampon de journal vide.
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(VecDeque::new())))
    }

    /// Ajoute une ligne (en supprimant les plus anciennes au-delà de la limite).
    pub fn push(&self, kind: LineKind, text: impl Into<String>) {
        if let Ok(mut lines) = self.0.lock() {
            lines.push_back(LogLine {
                kind,
                text: text.into(),
            });
            while lines.len() > MAX_LINES {
                lines.pop_front();
            }
        }
    }

    /// Copie instantanée des lignes pour l'affichage.
    fn snapshot(&self) -> Vec<LogLine> {
        self.0
            .lock()
            .map(|lines| lines.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Installe le logging de la GUI : filtre par verbosité + pont vers le tampon.
///
/// Précédence du niveau : `RUST_LOG` > `-v`/`-vv` > `config_level` (config) >
/// défaut. `config_level` provient de la configuration (section journalisation).
pub fn init(verbose: u8, config_level: &str, buffer: LogBuffer) {
    let default_level = match verbose {
        0 => config_level,
        1 => "debug",
        _ => "trace",
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::registry()
        .with(filter)
        .with(BufferLayer { buffer })
        .init();
}

/// Affiche le contenu du journal dans une zone scrollable.
pub fn render(ui: &mut egui::Ui, buffer: &LogBuffer) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for line in buffer.snapshot() {
                // Pastille « • » colorée selon le niveau (le glyphe ✓ manque dans la
                // police monospace embarquée → tofu ; la couleur porte le sens).
                let (prefix, color) = match line.kind {
                    LineKind::Success => ("\u{2022}", theme::SUCCESS),
                    LineKind::Info => ("\u{2022}", theme::GOLD),
                    LineKind::Error => ("\u{2022}", theme::LIVE),
                };
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(RichText::new(prefix).monospace().size(13.0).color(color));
                    ui.label(
                        RichText::new(line.text)
                            .monospace()
                            .size(13.0)
                            .color(theme::TEXT_MUTED),
                    );
                });
            }
        });
}

/// Couche `tracing` qui recopie chaque événement dans le tampon de journal.
struct BufferLayer {
    buffer: LogBuffer,
}

impl<S> Layer<S> for BufferLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        if visitor.message.is_empty() {
            return;
        }
        let kind = match *event.metadata().level() {
            Level::ERROR | Level::WARN => LineKind::Error,
            _ => LineKind::Info,
        };
        self.buffer.push(kind, visitor.message);
    }
}

/// Visiteur extrayant le message textuel d'un événement `tracing`.
#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        }
    }
}
