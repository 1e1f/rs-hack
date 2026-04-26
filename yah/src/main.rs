//! @arch:layer(cli)
//! @arch:role(discovery)
//! @arch:role(refactor)
//! @arch:depends_on(core, reason = "uses editor, operations, state, diff")
//! @arch:thread(main)
//!
//! CLI frontend for rs-hack. Parses clap commands and dispatches
//! to core library operations.
//!
//! @arch:see(architecture/multi-worktree-sync.md)
//!
//! @yah:relay(R006, "yah-ui: Tauri/React frontend port")
//! @yah:status(open)
//! @arch:see(architecture/yah-ui-implementation-guide.md)
//!
//! @yah:relay(R007, "yah-ui P1: design system + shell")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R006)
//! @arch:see(architecture/yah-ui-implementation-guide.md)
//!
//! @yah:relay(R008, "yah-ui P2: Board")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R006)
//! @arch:see(architecture/yah-ui-implementation-guide.md)
//!
//! @yah:relay(R009, "yah-ui P3: Architecture (mermaid)")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R006)
//! @arch:see(architecture/yah-ui-implementation-guide.md)
//!
//! @yah:relay(R010, "yah-ui P4: Agent")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R006)
//! @arch:see(architecture/yah-ui-implementation-guide.md)
//!
//! @yah:relay(R011, "yah-ui P5: splash assets + empty states")
//! @yah:status(open)
//! @yah:phase(P5)
//! @yah:parent(R006)
//! @arch:see(architecture/yah-ui-implementation-guide.md)
//!
//!
//! @yah:ticket(R007-T2, "yah-ui shared primitives: Pill, Glyph, Menu, SectionHeader")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R007)
//! @yah:handoff("Built four shared primitives in yah-ui/src/components/shared/: Pill (hue-keyed: open/active/handoff/review/bug/feature/task/epic, plus StatusPill, KindPill, KindBadge — all driven by --color-st-* tokens), Glyph (Icon line set + Glyph silhouette set ported from yah-design/project/data.jsx), Menu (anchored popover with click-outside via mousedown listener that ignores anchorRef) + MenuItem (leading/hint/danger slots), SectionHeader (illuminated drop-cap + small-caps eyebrow + horizontal rule, optional right slot for count badge). Refactored existing Pill.tsx off the generic Tailwind color tones (bg-blue, text-cyan, etc.) onto the new parchment palette. Typecheck clean, bun run build:css and build:js both green. Tailwind v4 emits text-st-{open|active|handoff|review|bug|feature|task|epic} + bg-vellum/70 + border-rule/50 + illum-cap/eyebrow/smallcaps utilities used by primitives.")
//! @yah:next("R007-T3 picks up next: replace TitleBar/RigSelector/RelaySelector/SplitModeToggle/TabStrip with versions consuming these primitives. Existing shell files (yah-ui/src/components/shell/*.tsx) still reference pre-yah-design Tailwind classes (bg-elevated, text-text-muted, bg-green/red) that don't exist in new globals.css — replace, don't extend.")
//! @yah:next("Visual verification of the primitives is deferred to R007-T3 — they're not wired into App.tsx. R007-T3's TitleBar+selectors will exercise Pill/Glyph/Menu against both themes.")
//! @yah:next("Note: yah-ui/src/components/board/TicketCard.tsx imports KindPill/StatusPill from shared/Pill — same export names + same prop shapes preserved, so it builds. But its inline Tailwind classes (bg-elevated, text-yellow, etc.) are still pre-yah-design — that's R008-T2 scope, not R007.")
//!
//! @yah:ticket(R007-T3, "yah-ui shell: TitleBar, RigSelector, RelaySelector, SplitModeToggle, TabStrip")
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R007)
//! @yah:handoff("Replaced 4 shell components + added SplitModeToggle, all built against the parchment palette + R007-T2 primitives (Pill/Glyph/Menu/SectionHeader/KindBadge). Extended Tab type to include the run cluster (terminal/preview/files/services) + Theme type. App.tsx now owns theme state (CSS-variable swap on documentElement.dataset.theme), splitMode state, and routes run-cluster tabs to a ComingSoon stub. Typecheck clean for new code (only pre-existing serve.ts ambient-types errors remain). bun build:js + build:css both green. Dev server boots cleanly.")
//! @yah:next("Visual sign-off: open localhost:5173, exercise theme toggle, rig + relay menus (⌘K filter), split-mode menu (cross-cluster pairings first), and the seven tabs (run-cluster shows ComingSoon).")
//! @yah:next("Once approved, archive R007-T2 + R007-T3 from the board (strips @hack lines from yah/src/main.rs).")
//!
//! @yah:ticket(R008-T1, "Board scaffold: 5 columns + dnd-kit drag-and-drop with transition rules")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R008)
//! @yah:handoff("Rewrote yah-ui/src/components/board/{Board,Column}.tsx onto the parchment palette. Board: 5 columns (Zones / Open / Active / Handoff / Review) with eyebrows, dnd-kit DndContext, ALLOWED_TARGETS transition matrix mirrored from CLAUDE.md (open->active; active->open|handoff|review; handoff->active|review; review->handoff). Drag start/end/cancel hooks track activeId; drop is rejected client-side when the target column is not in the source ticket's allowed set. Column: illuminated drop-cap initial + small-caps body + bordered count chip + italic eyebrow + filter button. Hover state uses color-mix(accent 9%, paper-2); disallowed columns dim to opacity-45; valid drop-hover gets accent border. Existing TicketCard left untouched (T2 scope). Typecheck clean apart from pre-existing serve.ts ambient-types errors. bun run build:css + build:js both green. Dev server boots.")
//! @yah:next("Visual sign-off: open localhost:5173, drag a ticket open->active->handoff->review (mock data has R012-T2 in active you can drop into handoff), confirm zones rejects, confirm a card from review only allows drop on handoff (other cols dim).")
//! @yah:next("Existing TicketCard renders with pre-yah-design Tailwind classes (bg-elevated, text-text-muted, etc.) that don't exist in globals.css — cards will look bare. That is R008-T2's problem, not T1's.")
//! @yah:verify("cd yah-ui && bun run build")
//! @yah:verify("cd yah-ui && bun run typecheck")
//!
//! @yah:ticket(R008-T2, "TicketCard + CardExpanded: relay rail, pennant, status pill, mixed-children smell")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R008)
//! @yah:handoff("Refactored TicketCard onto the parchment palette + extracted CardExpanded into its own file. TicketCard: cursor-grab article with bg-vellum, dnd-kit useDraggable on the whole card (chevron and footer buttons stopPropagation pointerdown so they don't start drags). Relay rail: 4px left strip with linear-gradient(transparent, accent, transparent), candle-rail animation + drop-shadow when live (relay + agent + status in {claimed,in-progress,handoff}). Pennant: top-right ribbon via clip-path polygon — 7x12 solid-accent for normal relays, 9x16 with five-stop gradient (accent-2 / accent / accent+vellum mix / accent / accent-2) for kind=epic. Header: chevron + KindBadge + id (with parent ↪ chip) + StatusPill (suppressed when STATUS_LABEL[status] === columnEyebrow) + mixed-children bug glyph + phase. Title in font-display 15px. Assignee row with midnight dot. Zone child counts inline (R008-T3 will replace with proper CountChip). CardExpanded: Handoff (serif prose) / Next steps (accent bullet) / Gotchas (st-bug warning icon) / Verify (mono code chip). Footer: file:line truncate + graph/agent/prompt buttons. Wired Column → TicketCard via columnEyebrow prop. Extended Ticket.childCounts with optional relays count to enable the mixed-children calculation; existing mock data left untouched (no mock ticket has mixed children, so the smell glyph won't render until real data comes in). Typecheck clean apart from pre-existing serve.ts ambient-types errors. bun run build:css + build:js green. Dev server boots.")
//! @yah:next("Visual sign-off at localhost:5173: open Board tab, expand a few cards (R012, R012-T2, R007, etc.), confirm relay rail + pennant on relays only, candle-rail animation on R012/R012-T2/R007 (the active/handoff agent-driven ones), epic pennant on R012 (kind=epic + isZone), drag a card to verify status pill suppression in the right column. I built clean but did NOT exercise the UI in a browser this turn — flag any visual regressions.")
//! @yah:next("If pennant overlaps the count chip on small relays or the chevron sits awkwardly under the rail, the easy tweak is the article's pl-3.5/pl-3 — increase by .5 if needed.")
//! @yah:verify("cd yah-ui && bun run typecheck")
//! @yah:verify("cd yah-ui && bun run build")
//!
//! @yah:ticket(R008-T3, "CountChip + EmptyColumn + FillerSplash")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R008)
//! @yah:handoff("Built CountChip (board/CountChip.tsx — serif numeral + small-caps label per chip, gilt rule divider via border-r, inherits hue text class), Splash (shared/Splash.tsx — placeholder vector ornaments per column-slug, prop-shape mirrors design return so R011/P5 can swap inner SVG for <img> without touching callers; legacy variant aliases scroll/lantern/camp/anvil/empty/signpost map to column slugs for run-tab + arch + agent reuse), EmptyColumn (board/EmptyColumn.tsx — themed Splash inside a dashed parchment frame, per-column caption/sub flavour) + FillerSplash (board/FillerSplash.tsx — bottom-of-column splash at width=140, opacity-45, pointer-events-none so it never blocks dnd-kit drops). Wired into Column.tsx: empty columns now render EmptyColumn instead of (empty) text; columns with 1-2 cards get FillerSplash pushed to the bottom. Refactored TicketCard's inline ZoneChildCounts to use CountChip. Typecheck clean apart from pre-existing serve.ts ambient-types errors. bun run build:css + build:js both green. I built clean but did NOT exercise the UI in a browser this turn — flag any visual regressions.")
//! @yah:next("Visual sign-off: open localhost:5173, switch to Board tab. Empty columns (Zones in mock data) should show themed dashed-frame splash with caption + sub. Columns with ≤2 cards should show a smaller bottom splash at low opacity. Drag a card and confirm dnd-kit drops still land — pointer-events-none on FillerSplash is critical.")
//! @yah:next("Zone child-counts on R012 should now render as serif numerals separated by gilt rules (CountChip) instead of inline separators. Hue tones: open=ochre, active=midnight-blue, handoff=sienna.")
//! @yah:next("Splash ornaments are deliberate vector placeholders — R011/P5 swaps them for the wayfarer linocut PNG set + adds a useThemeAndSet hook for light/dark + arcane/wayfarer set switching. The variant alias map (scroll/lantern/camp/anvil/empty/signpost) is already in place for the run-cluster + arch/agent surfaces P5 will wire.")
//! @yah:verify("cd yah-ui && bun run typecheck")
//! @yah:verify("cd yah-ui && bun run build")
//!
//! @yah:ticket(R009-T1, "GraphPane: mermaid theming, dot-grid, post-render polish, click handlers")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P3)
//! @yah:parent(R009)
//!
//! @yah:ticket(R009-T2, "ArchToolbar: RootSelector, DepthControl, EdgeKindFilters, PinnedViews, Legend")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R009)
//!
//! @yah:ticket(R009-T3, "NodeHoverCard + NodeActionMenu + cross-tab nav (jumpToFile, openInAgent)")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R009)
//!
//! @yah:ticket(R010-T1, "AgentView shell: SessionList, SessionPane, SessionHeader, StatusStrip")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R010)
//!
//! @yah:ticket(R010-T2, "Message components: User, Assistant, Thinking, Avatar (path:line linkification)")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R010)
//!
//! @yah:ticket(R010-T3, "Tool result frames: ReadResult, GrepResult, EditDiff, BashOutput + ToolFrame")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R010)
//!
//! @yah:ticket(R010-T4, "PromptBar + StreamingCursor + NoSession")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R010)
//!
//! @yah:ticket(R011-T1, "Splash component + wayfarer illustrations wired to Board/Arch/Agent/Run empty states")
//! @yah:status(open)
//! @yah:phase(P5)
//! @yah:parent(R011)

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use glob::glob;

mod operations;
mod visitor;
mod editor;
mod state;
mod diff;
mod path_resolver;
mod surgical;
mod worktrees;
mod arch;
mod mcp;

#[cfg(test)]
mod tests;

use operations::*;
use editor::RustEditor;
use state::{*, save_backup_nodes};
use diff::{print_diff, print_summary_diff, DiffStats};

#[derive(Parser)]
#[command(name = "yah")]
#[command(about = "Bulk refactor Rust: find/modify struct literals, enum variants, and function calls across your entire codebase")]
#[command(long_about = "AST-aware Rust refactoring tool that finds and modifies ALL usages across your codebase.

WHAT MAKES RS-HACK DIFFERENT:
  • Works on struct LITERALS (instantiation sites), not just definitions
  • One command updates 50 struct initializations scattered across many files
  • AST-aware: no false positives from comments or strings

COMMON USE CASES:
  Add a field to a struct + update ALL places it's instantiated:
    rs-hack add --name Config --field-name timeout --field-type Duration \\
               --field-value \"Duration::from_secs(30)\" --paths src --apply

  Find all places a struct is instantiated (not just where it's defined):
    rs-hack find --paths src --node-type struct-literal --name Config

  Remove a field from definition AND all 47 places it's used:
    rs-hack remove --name User --field-name deprecated_field --paths src --apply")]
#[command(after_help = "For detailed help on any command, use: rs-hack <COMMAND> --help

Examples:
  rs-hack find --help
  rs-hack add --help
  rs-hack rename --help")]
#[command(version)]
struct Cli {
    /// Use project-local state directory (.rs-hack) instead of ~/.rs-hack
    #[arg(long, global = true)]
    local_state: bool,

    /// Output format: "default", "diff", or "summary"
    #[arg(long, default_value = "default", global = true)]
    format: String,

    /// Show summary statistics after diff output
    #[arg(long, global = true)]
    summary: bool,

    /// Filter targets based on traits or attributes (e.g., "derives_trait:Clone", "derives_trait:Serialize,Debug")
    #[arg(long, global = true)]
    r#where: Option<String>,

    /// Exclude paths matching these patterns (can be used multiple times)
    #[arg(long, global = true, num_args = 0..)]
    exclude: Vec<String>,

    /// Limit the number of instances to modify (stops after N modifications)
    #[arg(long, global = true)]
    limit: Option<usize>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Refactor — AST-aware Rust code edits (yahh: short alias)
    #[command(subcommand)]
    Hack(HackCommands),

    /// Architecture knowledge graph - extract and query @arch: annotations (yaha: short alias)
    #[command(subcommand)]
    Arch(ArchCommands),

    /// hack-board — kanban, tickets, relays, SDLC rules (yahb: short alias)
    #[command(subcommand)]
    Board(BoardCommands),

    /// MCP server — JSON-RPC stdio bridge for AI agents
    Mcp {
        /// Transport (currently stdio only)
        #[arg(long, default_value = "stdio")]
        transport: String,
    },

    /// One-shot migration: rewrite legacy `@hack:` → `@yah:` in source, rename `.hack/` → `.yah/`
    Migrate {
        /// Workspace root to migrate
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Apply changes (default: dry-run prints what would change)
        #[arg(long)]
        apply: bool,
    },
}

#[derive(Subcommand)]
enum HackCommands {
    /// [LEGACY] Add a field to a struct - use 'yah hack add' instead
    #[command(hide = true)]
    #[command(after_help = "EXAMPLES:
    # Add field to struct definition only
    rs-hack add-struct-field --struct-name Config --field \"timeout: Duration\" --paths src --apply

    # Add field to definition AND all struct literals
    rs-hack add-struct-field --struct-name Config --field \"timeout: Duration\" --literal-default \"Duration::from_secs(30)\" --paths src --apply

    # Common case: field exists in struct, add to all literals
    rs-hack add-struct-field --struct-name Config --field timeout --literal-default \"Duration::from_secs(30)\" --paths src --apply

    # Insert field at specific position
    rs-hack add-struct-field --struct-name User --field \"created_at: DateTime\" --position first --paths src --apply
    rs-hack add-struct-field --struct-name User --field \"updated_at: DateTime\" --position \"after:created_at\" --paths src --apply

BEHAVIOR WITH --literal-default:
    Without --literal-default:
        Only modifies struct DEFINITION (adds field to struct declaration)

    With --literal-default:
        1. Tries to add field to struct definition (skips if already exists)
        2. ALWAYS adds field with default value to ALL struct literal expressions

    This is useful when:
        - Migrating existing code to use a new field
        - Field already exists in struct, but not all initializations use it
        - You want to ensure every struct creation includes the new field")]
    AddStructField {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the struct to modify
        #[arg(short, long)]
        struct_name: String,

        /// Field to add (e.g., "field_name: Type" or just "field_name" if using --literal-default)
        #[arg(short, long)]
        field: String,

        /// Where to insert: "first", "last", or "after:field_name"
        #[arg(short = 'P', long, default_value = "last")]
        position: String,

        /// Default value for struct literals (e.g., "None", "vec![]", "0")
        /// When provided: adds field to definition (idempotent) AND all literal expressions
        /// When omitted: only adds to definition
        #[arg(long)]
        literal_default: Option<String>,

        /// Output path (if specified, writes to new file instead of modifying in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// [DEPRECATED] Update an existing struct field (changes type/visibility) - use 'rs-hack update' instead
    #[command(hide = true)]
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack update --name <NAME> --field <FIELD>' instead

MIGRATION:
    Old: rs-hack update-struct-field --struct-name User --field \"pub email: String\"
    New: rs-hack update --name User --field \"pub email: String\"

EXAMPLES:
    # Update struct field type/visibility
    rs-hack update-struct-field --struct-name User --field \"pub email: String\" --paths src --apply

    # Update field type
    rs-hack update-struct-field --struct-name Config --field \"timeout: u64\" --paths src --apply")]
    UpdateStructField {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the struct to modify
        #[arg(short, long)]
        struct_name: String,

        /// Field definition (e.g., "field_name: NewType" or "pub field_name: Type")
        #[arg(short, long)]
        field: String,

        /// Output path (if specified, writes to new file instead of modifying in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    #[command(hide = true)]
    /// [DEPRECATED] Remove a field from struct definitions and all struct literal expressions - use 'rs-hack remove' instead
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack remove --name <NAME> --field-name <FIELD>' instead

MIGRATION:
    Old: rs-hack remove-struct-field --struct-name User --field-name email
    New: rs-hack remove --name User --field-name email

EXAMPLES:
    # Remove field from struct definition AND all struct literals
    rs-hack remove-struct-field --struct-name \"Config\" --field-name \"debug_mode\" --paths src --apply

    # Dry-run to preview changes (default behavior)
    rs-hack remove-struct-field --struct-name \"Config\" --field-name \"debug_mode\" --paths src

    # Remove field from enum variant (use Enum::Variant syntax)
    rs-hack remove-struct-field --struct-name \"View::Rectangle\" --field-name \"immediate_mode\" --paths src --apply

    # Remove field from literals only (keep in struct definition)
    rs-hack remove-struct-field --struct-name \"Config\" --field-name \"debug_mode\" --literal-only --paths src --apply

    # Works on multiple files in a directory
    rs-hack remove-struct-field --struct-name \"User\" --field-name \"deprecated_field\" --paths src --apply

WHAT IT DOES:
    This command removes a field in TWO places:
    1. From the struct/enum variant DEFINITION (e.g., struct Config { debug_mode: bool })
    2. From ALL struct literal expressions (e.g., Config { color: red, debug_mode: false })

    Both removals happen automatically - you don't need separate commands.

    With --literal-only: Only removes from struct literals, keeps the field in the definition.")]
    RemoveStructField {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the struct to modify (or \"EnumName::VariantName\" for enum variants)
        #[arg(short, long)]
        struct_name: String,

        /// Name of the field to remove
        #[arg(short = 'n', long)]
        field_name: String,

        /// Remove field from struct literal expressions only, not the definition
        #[arg(long)]
        literal_only: bool,

        /// Output path (if specified, writes to new file instead of modifying in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// DEPRECATED: Use add-struct-field --literal-only instead
    /// Add a field to struct literal expressions (initialization)
    #[deprecated]
    #[command(hide = true)]
    AddStructLiteralField {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the struct to target
        #[arg(short, long)]
        struct_name: String,

        /// Field with value to add (e.g., "return_type: None")
        #[arg(short, long)]
        field: String,

        /// Where to insert: "first", "last", "after:field_name", or "before:field_name"
        #[arg(short = 'P', long, default_value = "last")]
        position: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// [LEGACY] Add a variant to an enum - use 'rs-hack add' instead
    #[command(hide = true)]
    AddEnumVariant {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the enum to modify
        #[arg(short, long)]
        enum_name: String,

        /// Variant to add (e.g., "NewVariant" or "NewVariant { field: Type }")
        #[arg(short, long)]
        variant: String,

        /// Where to insert: "first", "last", or "after:VariantName"
        #[arg(short = 'P', long, default_value = "last")]
        position: String,

        /// Output path (if specified, writes to new file instead of modifying in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    #[command(hide = true)]
    /// [DEPRECATED] Update an existing enum variant - use 'rs-hack update' instead
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack update --name <NAME> --variant <VARIANT>' instead

MIGRATION:
    Old: rs-hack update-enum-variant --enum-name Status --variant \"Draft { created_at: u64 }\"
    New: rs-hack update --name Status --variant \"Draft { created_at: u64 }\"

EXAMPLES:
    # Update enum variant
    rs-hack update-enum-variant --enum-name Status --variant \"Draft { created_at: u64 }\" --paths src --apply

    # Add field to enum variant
    rs-hack update-enum-variant --enum-name Status --variant \"Active { user_id: u32 }\" --paths src --apply")]
    UpdateEnumVariant {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the enum to modify
        #[arg(short, long)]
        enum_name: String,

        /// Variant definition (e.g., "VariantName { new_field: Type }")
        #[arg(short, long)]
        variant: String,

        /// Output path (if specified, writes to new file instead of modifying in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    #[command(hide = true)]
    /// [DEPRECATED] Remove a variant from an enum - use 'rs-hack remove' instead
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack remove --name <NAME> --variant <VARIANT>' instead

MIGRATION:
    Old: rs-hack remove-enum-variant --enum-name Status --variant-name Draft
    New: rs-hack remove --name Status --variant Draft")]
    RemoveEnumVariant {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the enum to modify
        #[arg(short, long)]
        enum_name: String,

        /// Name of the variant to remove
        #[arg(short = 'n', long)]
        variant_name: String,

        /// Output path (if specified, writes to new file instead of modifying in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    #[command(hide = true)]
    /// [DEPRECATED] Rename an enum variant across the codebase - use 'rs-hack rename' instead
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack rename --name <NAME> --to <NEW_NAME>' instead

MIGRATION:
    Old: rs-hack rename-enum-variant --enum-name Status --old-variant Draft --new-variant Pending
    New: rs-hack rename --name Status::Draft --to Pending

EXAMPLES:
    # Rename enum variant
    rs-hack rename-enum-variant --enum-name Status --old-variant Draft --new-variant Pending --paths src --apply")]
    RenameEnumVariant {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the enum containing the variant
        #[arg(short, long)]
        enum_name: String,

        /// Current name of the variant
        #[arg(short = 'o', long)]
        old_variant: String,

        /// New name for the variant
        #[arg(short = 'n', long)]
        new_variant: String,

        /// Optional fully-qualified path to the enum (e.g., "crate::compiler::types::IRValue")
        /// When provided, enables safe matching of fully qualified paths by tracking use statements
        #[arg(long)]
        enum_path: Option<String>,

        /// Edit mode: 'surgical' (default, preserves formatting) or 'reformat' (uses prettyplease)
        #[arg(long, default_value = "surgical")]
        edit_mode: String,

        /// Validate mode: check for remaining references without making changes
        #[arg(long)]
        validate: bool,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    #[command(hide = true)]
    /// [DEPRECATED] Rename a function across the codebase - use 'rs-hack rename' instead
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack rename --name <NAME> --to <NEW_NAME>' instead

MIGRATION:
    Old: rs-hack rename-function --old-name process_v2 --new-name process
    New: rs-hack rename --name process_v2 --to process

EXAMPLES:
    # Rename function
    rs-hack rename-function --old-name process_v2 --new-name process --paths src --apply")]
    RenameFunction {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Current name of the function
        #[arg(short = 'o', long)]
        old_name: String,

        /// New name for the function
        #[arg(short = 'n', long)]
        new_name: String,

        /// Optional fully-qualified path to the function (e.g., "crate::utils::process_v2")
        /// When provided, enables safe matching of fully qualified paths by tracking use statements
        #[arg(long)]
        function_path: Option<String>,

        /// Edit mode: 'surgical' (default, preserves formatting) or 'reformat' (uses prettyplease)
        #[arg(long, default_value = "surgical")]
        edit_mode: String,

        /// Validate mode: check for remaining references without making changes
        #[arg(long)]
        validate: bool,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Rename functions, enum variants - updates ALL call sites (see: rs-hack rename --help)
    #[command(display_order = 3)]
    #[command(after_help = "EXAMPLES:
    # Rename function
    rs-hack rename --name process_v2 --to process --paths src --apply

    # Rename enum variant (with :: syntax)
    rs-hack rename --name Status::Draft --to Pending --paths src --apply

    # Rename enum variant with qualified path for disambiguation
    rs-hack rename --name Status::Draft --to Pending --enum-path \"types::Status\" --paths src --apply

    # Validation mode (check for remaining references)
    rs-hack rename --name Status::Draft --to Pending --validate --paths src

    # Use reformat mode instead of surgical (preserves formatting less precisely)
    rs-hack rename --name process_v2 --to process --edit-mode reformat --paths src --apply

AUTO-DETECTION:
    The command auto-detects whether to rename a function or enum variant:
    - If --name contains :: (e.g., Status::Draft), it's an enum variant rename
    - Otherwise, it searches the codebase to determine if it's a function or enum variant
    - If both exist with the same name, you'll be asked to disambiguate with :: syntax

ENUM VARIANT SYNTAX:
    Use EnumName::VariantName to specify an enum variant:
      rs-hack rename --name Status::Draft --to Pending --paths src --apply

    This works for both the target name (--name) specification.

QUALIFIED PATHS:
    Use --enum-path or --function-path to provide fully-qualified paths for disambiguation:
      --enum-path \"crate::types::Status\"
      --function-path \"crate::utils::process_v2\"

EDIT MODES:
    - surgical (default): Preserves formatting precisely, makes minimal changes
    - reformat: Uses prettyplease to reformat modified code

VALIDATION:
    Use --validate to check for remaining references without making changes:
      rs-hack rename --name old_name --to new_name --validate --paths src

NOTES:
    - Use --name <NAME> to specify the target to rename
    - Use --to <NEW_NAME> to specify the new name
    - For enum variants, use :: syntax (EnumName::VariantName)
    - The command performs renames across definitions and all usages")]
    Rename {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Target to rename (function name or EnumName::VariantName for enum variants)
        #[arg(short, long)]
        name: String,

        /// New name
        #[arg(short = 't', long)]
        to: String,

        /// Optional qualified enum path (e.g., "types::Status") for enum variant renames
        #[arg(long)]
        enum_path: Option<String>,

        /// Optional qualified function path (e.g., "crate::utils::process_v2") for function renames
        #[arg(long)]
        function_path: Option<String>,

        /// Semantic kind for grouping related node types (struct, function, enum, match, identifier, type, macro, const, trait, mod, use)
        #[arg(short = 'k', long, conflicts_with = "node_type")]
        kind: Option<String>,

        /// Specific AST node type for granular control (function-call, identifier, etc.)
        #[arg(short = 'T', long, conflicts_with = "kind")]
        node_type: Option<String>,

        /// Edit mode: 'surgical' (default, preserves formatting) or 'reformat' (uses prettyplease)
        #[arg(long, default_value = "surgical")]
        edit_mode: String,

        /// Validate mode: check for remaining references without making changes
        #[arg(long)]
        validate: bool,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    #[command(hide = true)]
    /// [DEPRECATED] Add a match arm for a specific pattern - use 'rs-hack add' instead
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack add --match-arm <PATTERN> --body <BODY>' instead

MIGRATION:
    Old: rs-hack add-match-arm --pattern \"Status::Archived\" --body \"todo!()\"
    New: rs-hack add --match-arm \"Status::Archived\" --body \"todo!()\" --paths src --apply

    Old: rs-hack add-match-arm --auto-detect --enum-name Status --body \"todo!()\"
    New: rs-hack add --auto-detect --enum-name Status --body \"todo!()\" --paths src --apply")]
    AddMatchArm {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Pattern to match (e.g., "MyEnum::NewVariant"). Not required with --auto-detect
        #[arg(short = 'P', long)]
        pattern: Option<String>,

        /// Body of the match arm (e.g., "todo!()" or "println!(\"handled\")")
        #[arg(short, long)]
        body: String,

        /// Optional: function name containing the match
        #[arg(short, long)]
        function: Option<String>,

        /// Auto-detect and add all missing enum variants
        #[arg(long)]
        auto_detect: bool,

        /// Enum name for auto-detection (required if --auto-detect is used)
        #[arg(short, long)]
        enum_name: Option<String>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    #[command(hide = true)]
    /// [DEPRECATED] Update an existing match arm - use 'rs-hack update' instead
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack update --match-arm <PATTERN> --body <BODY>' instead

MIGRATION:
    Old: rs-hack update-match-arm --pattern \"Status::Draft\" --body \"\\\"pending\\\".to_string()\"
    New: rs-hack update --match-arm \"Status::Draft\" --body \"\\\"pending\\\".to_string()\" --paths src --apply")]
    UpdateMatchArm {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Pattern to match (e.g., "MyEnum::Variant")
        #[arg(short = 'P', long)]
        pattern: String,

        /// New body for the match arm
        #[arg(short, long)]
        body: String,

        /// Optional: function name containing the match
        #[arg(short, long)]
        function: Option<String>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    #[command(hide = true)]
    /// [DEPRECATED] Remove a match arm - use 'rs-hack remove' instead
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack remove --match-arm <PATTERN>' instead

MIGRATION:
    Old: rs-hack remove-match-arm --pattern \"Status::Deleted\"
    New: rs-hack remove --match-arm \"Status::Deleted\" --paths src --apply")]
    RemoveMatchArm {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Pattern to remove (e.g., "MyEnum::Variant")
        #[arg(short = 'P', long)]
        pattern: String,

        /// Optional: function name containing the match
        #[arg(short, long)]
        function: Option<String>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Batch operation from JSON or YAML specification
    Batch {
        /// Path to JSON or YAML file with batch operations
        #[arg(short, long)]
        spec: PathBuf,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Find definitions AND all usages across files (see: rs-hack find --help)
    #[command(display_order = 1)]
    #[command(after_help = "EXAMPLES:
    # NEW: Search all node types when you don't know what you're looking for
    rs-hack find --paths src --name Rectangle

    # Find all struct literal expressions for a specific struct
    rs-hack find --paths src --node-type struct-literal --name Shadow

    # Find all calls to unwrap() method
    rs-hack find --paths src --node-type method-call --name unwrap

    # Find all eprintln! debug statements
    rs-hack find --paths src --node-type macro-call --name eprintln

    # Find enum variant usages
    rs-hack find --paths src --node-type enum-usage --name \"Operator::Error\"

    # NEW: Enum variant filtering - all four patterns work:
    # 1. Find any enum with Rectangle variant
    rs-hack find --paths src --node-type enum --variant Rectangle
    # 2. Find View enum, show only Rectangle variant
    rs-hack find --paths src --node-type enum --name View --variant Rectangle
    # 3. Same using :: syntax
    rs-hack find --paths src --node-type enum --name View::Rectangle
    # 4. Wildcard: any enum with Rectangle variant
    rs-hack find --paths src --node-type enum --name \"*::Rectangle\"

    # Find nodes containing specific text
    rs-hack find --paths src --node-type struct-literal --content-filter \"[SHADOW RENDER]\"

    # Get JSON output (useful for scripting)
    rs-hack find --paths src --node-type function --name process --format json

    # Get just file locations (grep-like output)
    rs-hack find --paths src --node-type method-call --name unwrap --format locations

    # Search multiple files with glob patterns
    rs-hack find --paths \"src/**/*.rs\" --node-type struct --name Config

    # Include documentation comments in output
    rs-hack find --paths src --node-type function --name main --include-comments true

OUTPUT FORMATS:
    snippets    Show full code snippets with file locations (default, most readable)
    locations   Show only file:line:column (grep-style, good for scripting)
    json        JSON output with all metadata (for programmatic use)")]
    Find {
        /// Path to Rust file(s) - supports multiple paths and glob patterns (e.g., "tests/*.rs")
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Semantic kind for grouping related node types (struct, function, enum, match, identifier, type, macro, const, trait, mod, use)
        #[arg(short = 'k', long, conflicts_with = "node_type")]
        kind: Option<String>,

        /// Type of node: Expression-level: "struct-literal", "match-arm", "enum-usage", "function-call", "method-call", "macro-call", "identifier", "type-ref". Definition-level: "struct", "enum", "function", "impl-method", "trait", "const", "static", "type-alias", "mod". Omit to search all types.
        #[arg(short = 't', long, conflicts_with = "kind")]
        node_type: Option<String>,

        /// Filter by name (e.g., "Shadow", "Operator::Error", "unwrap", "eprintln", "Vec")
        #[arg(short, long)]
        name: Option<String>,

        /// Filter enum variants by name (only valid with --node-type enum)
        #[arg(short = 'v', long)]
        variant: Option<String>,

        /// Filter by content - only show nodes whose source contains this string (e.g., "[SHADOW RENDER]")
        #[arg(short = 'c', long)]
        content_filter: Option<String>,

        /// Find all occurrences of a field across struct definitions, enum variants, and struct literals
        #[arg(short = 'F', long)]
        field_name: Option<String>,

        /// Include preceding comments (doc and regular) in output
        #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
        include_comments: bool,

        /// Output format: "json", "locations", "snippets"
        #[arg(short = 'f', long, default_value = "snippets")]
        format: String,
    },

    /// [LEGACY] Add derive macros - use 'rs-hack add' instead
    #[command(hide = true)]
    AddDerive {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Type of target: "struct" or "enum"
        #[arg(short = 't', long)]
        target_type: String,

        /// Name of the struct or enum
        #[arg(short, long)]
        name: String,

        /// Derives to add (comma-separated, e.g., "Clone,Debug,Serialize")
        #[arg(short, long)]
        derives: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// [LEGACY] Add a method to an impl block - use 'rs-hack add' instead
    #[command(hide = true)]
    AddImplMethod {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the type (struct/enum) that the impl is for
        #[arg(short = 't', long)]
        target: String,

        /// Method definition (e.g., "pub fn get_id(&self) -> u64 { self.id }")
        #[arg(short, long)]
        method: String,

        /// Where to insert: "first", "last", "after:method_name", or "before:method_name"
        #[arg(short = 'P', long, default_value = "last")]
        position: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// [LEGACY] Add a use statement - use 'rs-hack add' instead
    #[command(hide = true)]
    AddUse {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Use path (e.g., "std::collections::HashMap" or "serde::Serialize")
        #[arg(short = 'u', long)]
        use_path: String,

        /// Where to insert: "first", "last", or "after:module"
        #[arg(short = 'P', long, default_value = "last")]
        position: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Add fields, variants, methods, derives, match arms - updates ALL usages (see: rs-hack add --help)
    #[command(display_order = 2)]
    #[command(after_help = "COMMON USE CASE - Add field to ALL struct literal instantiations:
    # Add a field to every place MyStruct { ... } appears in your codebase
    rs-hack add --name MyStruct --field-name new_field --field-value \"None\" --paths src --apply

    # For enum variants like View::Container, use the full path
    rs-hack add --name \"View::Container\" --field-name style --field-value \"None\" --paths src --apply

EXAMPLES:
    # Add field to struct definition only (no --field-value)
    rs-hack add --name User --field-name email --field-type String --paths src --apply

    # Add field to definition AND all literals (with --field-value)
    rs-hack add --name Config --field-name timeout --field-type Duration \\
               --field-value \"Duration::from_secs(30)\" --paths src --apply

    # Add field to literals only, not definition (--literal-only)
    rs-hack add --name Config --field-name timeout --field-value \"Duration::from_secs(30)\" \\
               --literal-only --paths src --apply

    # Add enum variant
    rs-hack add --name Status --variant \"Archived\" --paths src --apply

    # Add impl method
    rs-hack add --name User --method \"pub fn new() -> Self { Self { id: 0 } }\" --paths src --apply

    # Add derive macros
    rs-hack add --name User --derive \"Clone,Debug\" --paths src --apply

    # Add use statement (no --name required)
    rs-hack add --use \"serde::Serialize\" --paths src --apply

    # Add ..Default::default() to struct literals that need it
    rs-hack add --name Config --default-rest --paths src --apply

    # Add custom base expression (e.g., ..other_instance)
    rs-hack add --name Config --base \"existing_config\" --paths src --apply

AUTO-DETECTION:
    The command auto-detects what to add based on which flags you provide:
    - --field-name + --field-type: Add to struct definition
    - --field-name + --field-value: Add to all struct literals
    - --field-name + --field-type + --field-value: Add to both
    - --variant: Add enum variant
    - --method: Add impl method
    - --derive: Add derive macro
    - --use: Add use statement
    - --default-rest: Add ..Default::default() to struct literals
    - --base: Add custom base expression (..expr) to struct literals

ENUM VARIANT SYNTAX:
    For enum struct variants, use \"EnumName::VariantName\" syntax:
    rs-hack add --name \"View::Container\" --field-name shadow --field-value \"None\" --paths src

MATCH ARMS (two modes):
    Mode 1 - Auto-detect ALL missing arms (enum must be in scanned files):
    rs-hack add --auto-detect --enum-name Status --body \"todo!()\" --paths src --apply

    Mode 2 - Add ONE specific arm (works with external enums):
    rs-hack add --match-arm \"Status::Archived\" --body \"\\\"archived\\\".to_string()\" --paths src --apply

    Note: --auto-detect ignores --match-arm. Use one mode or the other.

NOTES:
    - Use --name <NAME> to specify the target struct/enum/impl (not needed for --use)
    - Position can be controlled with --position (first, last, after:name, before:name)")]
    Add {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Target name (struct/enum/impl name) - not required for --use
        #[arg(short, long)]
        name: Option<String>,

        /// [DEPRECATED] Field definition to add (e.g., \"email: String\"). Use --field-name + --field-type + --field-value instead.
        #[arg(short, long, conflicts_with_all = ["field_name", "field_type", "field_value"])]
        field: Option<String>,

        /// Field name (e.g., \"email\"). Use with --field-type and/or --field-value
        #[arg(long)]
        field_name: Option<String>,

        /// Field type (e.g., \"String\", \"Option<i32>\"). Adds to struct definition.
        #[arg(long, requires = "field_name")]
        field_type: Option<String>,

        /// Field value (e.g., \"None\", \"0\", \"vec![]\"). Adds to struct literals.
        #[arg(long, requires = "field_name")]
        field_value: Option<String>,

        /// Variant definition to add (e.g., \"Archived\" or \"Draft { id: u32 }\")
        #[arg(short = 'v', long)]
        variant: Option<String>,

        /// Method definition to add (e.g., \"pub fn new() -> Self { ... }\")
        #[arg(short, long)]
        method: Option<String>,

        /// Derive macros to add (comma-separated, e.g., \"Clone,Debug,Serialize\")
        #[arg(short = 'd', long)]
        derive: Option<String>,

        /// Use statement path (e.g., \"std::collections::HashMap\")
        #[arg(short = 'u', long)]
        r#use: Option<String>,

        /// Match arm pattern for adding a SINGLE arm (e.g., \"Status::Archived\")
        /// Mutually exclusive with --auto-detect. Use this for external enums
        #[arg(long)]
        match_arm: Option<String>,

        /// Body of the match arm (e.g., \"\\\"archived\\\".to_string()\")
        #[arg(long)]
        body: Option<String>,

        /// Function containing the match expression (optional, limits scope)
        #[arg(long)]
        function: Option<String>,

        /// Auto-detect ALL missing match arms from enum definition (enum must be in scanned files)
        /// Ignores --match-arm; finds enum variants and adds all missing ones
        #[arg(long)]
        auto_detect: bool,

        /// Enum name for --auto-detect mode (required with --auto-detect)
        #[arg(long)]
        enum_name: Option<String>,

        /// Documentation comment text (use with --name and --node-type)
        #[arg(long)]
        doc_comment: Option<String>,

        /// Semantic kind for grouping related node types (struct, function, enum, match, identifier, type, macro, const, trait, mod, use)
        #[arg(short = 'k', long, conflicts_with = "node_type")]
        kind: Option<String>,

        /// Specific AST node type for granular control (struct, struct-literal, function, function-call, method-call, impl-method, enum, enum-usage, match-arm, identifier, type-ref, type-alias, macro-call, const, static, trait, mod, use)
        #[arg(short = 't', long, conflicts_with = "kind")]
        node_type: Option<String>,

        /// Default value for struct literals (only with --field)
        #[arg(long)]
        literal_default: Option<String>,

        /// Only affect literals, not definitions (only with --field)
        #[arg(long)]
        literal_only: bool,

        /// Add ..Default::default() to all struct literals that don't have a base expression
        /// Use with --name to specify the struct, and --kind struct for struct literals
        #[arg(long)]
        default_rest: bool,

        /// Custom base expression for struct literals (e.g., "other_instance" adds ..other_instance)
        /// Use "default" as shorthand for "Default::default()"
        #[arg(long, conflicts_with = "default_rest")]
        base: Option<String>,

        /// Function or method name to add argument to (e.g., "create_user", "process")
        #[arg(long)]
        call: Option<String>,

        /// Argument expression to add (e.g., "None", "ctx.clone()", "Default::default()")
        #[arg(long)]
        arg: Option<String>,

        /// Position for new argument: "first", "last", or index number (0-based)
        #[arg(long, default_value = "last")]
        arg_position: String,

        /// Filter to "function" or "method" calls only (default: both)
        #[arg(long)]
        call_type: Option<String>,

        /// Filter call sites by content substring
        #[arg(long)]
        content_filter: Option<String>,

        /// Where to insert: \"first\", \"last\", \"after:name\", or \"before:name\"
        #[arg(short = 'P', long, default_value = "last")]
        position: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Remove fields, variants, methods, derives, match arms - from ALL usages (see: rs-hack remove --help)
    #[command(display_order = 4)]
    #[command(after_help = "EXAMPLES:
    # Remove struct field (from definition AND all literals)
    rs-hack remove --name User --field-name email --paths src --apply

    # Remove enum variant field (use EnumName::VariantName syntax)
    rs-hack remove --name View::Rectangle --field-name color --paths src --apply

    # Remove field from literals only (keep in definition)
    rs-hack remove --name Config --field-name debug_mode --literal-only --paths src --apply

    # Remove enum variant
    rs-hack remove --name Status --variant Draft --paths src --apply

    # Remove derive macro
    rs-hack remove --name User --derive Clone --paths src --apply

    # Remove impl method
    rs-hack remove --name User --method get_email --paths src --apply

AUTO-DETECTION:
    The command auto-detects what to remove based on which flag you provide:
    - --field-name: Remove struct field (or enum variant field with :: syntax)
    - --variant: Remove enum variant
    - --method: Remove impl method
    - --derive: Remove derive macro

    If the target (--name) is not found, the command will search the codebase
    and show hints about what exists and how to fix the command.

ENUM VARIANT FIELDS:
    To remove a field from an enum variant, use the EnumName::VariantName syntax:
      rs-hack remove --name View::Rectangle --field-name color --paths src --apply

    This works on both the variant definition AND all enum variant literals.
    Use --literal-only to only remove from literals.

NOTES:
    - All --name values specify the target struct/enum/impl
    - For enum variant fields, --name uses :: syntax (EnumName::VariantName)
    - Removing struct fields affects both definitions and literals (unless --literal-only)")]
    Remove {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Target name (struct/enum/impl name, or EnumName::VariantName for variant fields)
        #[arg(short, long)]
        name: Option<String>,

        /// Field name to remove (use with struct/enum variant)
        #[arg(short = 'F', long)]
        field_name: Option<String>,

        /// Variant name to remove (use with enum)
        #[arg(short = 'v', long)]
        variant: Option<String>,

        /// Method name to remove (use with impl)
        #[arg(short, long)]
        method: Option<String>,

        /// Derive macro to remove (use with struct/enum)
        #[arg(short = 'd', long)]
        derive: Option<String>,

        /// Match arm pattern to remove (e.g., \"Status::Deleted\")
        #[arg(long)]
        match_arm: Option<String>,

        /// Function containing the match expression (optional, for --match-arm)
        #[arg(long)]
        function: Option<String>,

        /// Remove documentation comment (boolean flag, use with --name and --node-type)
        #[arg(long)]
        doc_comment: bool,

        /// Semantic kind for grouping related node types (struct, function, enum, match, identifier, type, macro, const, trait, mod, use)
        #[arg(short = 'k', long, conflicts_with = "node_type")]
        kind: Option<String>,

        /// Specific AST node type for granular control (struct, struct-literal, function, function-call, method-call, impl-method, enum, enum-usage, match-arm, identifier, type-ref, type-alias, macro-call, const, static, trait, mod, use)
        #[arg(short = 't', long, conflicts_with = "kind")]
        node_type: Option<String>,

        /// Only affect literals, not definitions (only with --field-name)
        #[arg(long)]
        literal_only: bool,

        /// Function or method name to remove argument from (e.g., "create_user", "process")
        #[arg(long)]
        call: Option<String>,

        /// Index of argument to remove (0-based)
        #[arg(long)]
        arg_index: Option<usize>,

        /// Filter to "function" or "method" calls only (default: both)
        #[arg(long)]
        call_type: Option<String>,

        /// Filter call sites by content substring
        #[arg(long)]
        content_filter: Option<String>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Update fields, variants, match arms - modifies ALL usages (see: rs-hack update --help)
    #[command(display_order = 5)]
    #[command(after_help = "EXAMPLES:
    # Update struct field type/visibility
    rs-hack update --name User --field \"pub email: String\" --paths src --apply

    # Update enum variant
    rs-hack update --name Status --variant \"Draft { created_at: u64 }\" --paths src --apply

    # Update struct field (change type)
    rs-hack update --name Config --field \"timeout: u64\" --paths src --apply

    # Update enum variant (add field)
    rs-hack update --name Status --variant \"Active { user_id: u32 }\" --paths src --apply

AUTO-DETECTION:
    The command auto-detects what to update based on which flag you provide:
    - --field: Update struct field (changes type/visibility)
    - --variant: Update enum variant (changes fields/type)

    If the target (--name) is not found, the command will search the codebase
    and show hints about what exists and how to fix the command.

WHAT IT DOES:
    - For struct fields: Updates the field definition (type, visibility, etc.)
    - For enum variants: Updates the variant definition (changes structure)

NOTES:
    - Use --name <NAME> to specify the target struct/enum
    - For --field, provide the new field definition (e.g., \"pub email: String\")
    - For --variant, provide the new variant definition (e.g., \"Draft { created_at: u64 }\")
    - The field/variant name is parsed from the definition you provide")]
    Update {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Target name (struct/enum name)
        #[arg(short, long)]
        name: Option<String>,

        /// [DEPRECATED] New field definition (e.g., \"pub email: String\"). Use --field-name + --field-type instead.
        #[arg(short, long, conflicts_with_all = ["field_name", "field_type"])]
        field: Option<String>,

        /// Field name to update
        #[arg(long)]
        field_name: Option<String>,

        /// New field type (e.g., \"String\", \"pub Option<i32>\")
        #[arg(long, requires = "field_name")]
        field_type: Option<String>,

        /// New variant definition (e.g., \"Draft { created_at: u64 }\")
        #[arg(short = 'v', long)]
        variant: Option<String>,

        /// Match arm pattern to update (e.g., \"Status::Draft\")
        #[arg(long)]
        match_arm: Option<String>,

        /// New body for the match arm (e.g., \"\\\"pending\\\".to_string()\") - required with --match-arm
        #[arg(long)]
        body: Option<String>,

        /// Function containing the match expression (optional, for --match-arm)
        #[arg(long)]
        function: Option<String>,

        /// Documentation comment text (use with --name and --node-type)
        #[arg(long)]
        doc_comment: Option<String>,

        /// Semantic kind for grouping related node types (struct, function, enum, match, identifier, type, macro, const, trait, mod, use)
        #[arg(short = 'k', long, conflicts_with = "node_type")]
        kind: Option<String>,

        /// Specific AST node type for granular control (struct, struct-literal, function, function-call, method-call, impl-method, enum, enum-usage, match-arm, identifier, type-ref, type-alias, macro-call, const, static, trait, mod, use)
        #[arg(short = 't', long, conflicts_with = "kind")]
        node_type: Option<String>,

        /// Function or method name to update argument in (e.g., "create_user", "process")
        #[arg(long)]
        call: Option<String>,

        /// Index of argument to update (0-based)
        #[arg(long)]
        arg_index: Option<usize>,

        /// New argument expression (e.g., "None", "ctx.clone()")
        #[arg(long)]
        arg: Option<String>,

        /// Filter to "function" or "method" calls only (default: both)
        #[arg(long)]
        call_type: Option<String>,

        /// Filter call sites by content substring
        #[arg(long)]
        content_filter: Option<String>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Show history of rs-hack runs
    History {
        /// Number of recent runs to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Revert a specific run
    Revert {
        /// Run ID to revert (from history)
        run_id: String,

        /// Force revert even if files changed since
        #[arg(long)]
        force: bool,
    },

    /// Clean old state data
    Clean {
        /// Keep runs from last N days
        #[arg(long, default_value = "30")]
        keep_days: u32,
    },

    /// Bulk modify expressions (comment out unwraps, remove debug macros, replace calls)
    #[command(after_help = "WHAT IS TRANSFORM?
    Transform is for bulk code cleanup and refactoring of EXPRESSIONS (how code is used).
    Unlike add/remove/update which modify DEFINITIONS (structs, enums, functions),
    transform finds and modifies expressions like method calls, macros, and literals.

    Think: 'find + sed' but AST-aware.

WHEN TO USE TRANSFORM:
    - Comment out all .unwrap() calls for safety audit
    - Remove all debug println!/eprintln! statements
    - Replace deprecated function calls across codebase
    - Clean up todo!() placeholders
    - Remove test-only code markers

WHEN TO USE OTHER COMMANDS:
    - add/remove/update: Modify struct/enum definitions (add fields, change types)
    - rename: Change names everywhere (rename functions, variants)
    - transform: Bulk modify how code is called/used (this command!)

ACTIONS:
    comment     Wrap code in /* ... */ (preserves it for reference)
    remove      Delete code entirely
    replace     Swap with new code (use --with to specify replacement)

SUPPORTED NODE TYPES:

Expression-level nodes (8 types):
    struct-literal      Struct initialization (e.g., Config { field: value })
    match-arm           Match arm pattern and body
    enum-usage          Enum variant usage (e.g., Status::Active)
    function-call       Function call (e.g., process_data())
    method-call         Method call (e.g., value.unwrap())
    macro-call          Macro invocation (e.g., println!(), vec![])
    identifier          Variable or type identifier
    type-ref            Type reference in annotations

Definition-level nodes (9 types):
    struct              Struct definition
    enum                Enum definition
    function            Function definition
    impl-method         Method in impl block
    trait               Trait definition
    const               Const item
    static              Static item
    type-alias          Type alias
    mod                 Module definition

EXAMPLES:
    # Comment out all unwrap() calls
    rs-hack transform --paths src --node-type method-call --name unwrap --action comment --apply

    # Remove all eprintln! debug statements
    rs-hack transform --paths src --node-type macro-call --name eprintln --action remove --apply

    # Replace a specific function call
    rs-hack transform --paths src --node-type function-call --name old_func --action replace --with new_func --apply

    # Remove all struct literals containing a specific value
    rs-hack transform --paths src --node-type struct-literal --content-filter \"[SHADOW RENDER]\" --action remove --apply

    # Comment out all TODO match arms
    rs-hack transform --paths src --node-type match-arm --content-filter \"todo!()\" --action comment --apply

    # Preview changes before applying (default dry-run)
    rs-hack transform --paths src --node-type method-call --name unwrap --action comment")]
    Transform {
        /// Path to Rust file(s) - supports multiple paths and glob patterns (e.g., "src/**/*.rs")
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Type of node (see SUPPORTED NODE TYPES above for full list)
        #[arg(short = 't', long)]
        node_type: String,

        /// Filter by name (e.g., "eprintln", "unwrap", "Config")
        #[arg(short, long)]
        name: Option<String>,

        /// Filter by content - only transform nodes containing this string
        #[arg(short = 'c', long)]
        content_filter: Option<String>,

        /// Action to perform: "comment", "remove", or "replace"
        #[arg(short, long)]
        action: String,

        /// Replacement code (required if action is "replace")
        #[arg(short = 'w', long)]
        with: Option<String>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    #[command(hide = true)]
    /// [DEPRECATED] Add documentation comment to an item - use 'rs-hack add' instead
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack add --name <NAME> --node-type <TYPE> --doc-comment <TEXT>' instead

MIGRATION:
    Old: rs-hack add-doc-comment --target-type struct --name User --doc-comment \"User model\"
    New: rs-hack add --name User --node-type struct --doc-comment \"User model\" --paths src --apply")]
    AddDocComment {
        /// Path to Rust file(s) - supports multiple paths and glob patterns
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Type of target: "struct", "enum", "function", "field", "variant"
        #[arg(short = 't', long)]
        target_type: String,

        /// Name of the target (e.g., "User", "Status::Draft", "User::id")
        #[arg(short, long)]
        name: String,

        /// Documentation comment text (without /// prefix)
        #[arg(short = 'd', long)]
        doc_comment: String,

        /// Comment style: "line" (///) or "block" (/** */)
        #[arg(long, default_value = "line")]
        style: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    #[command(hide = true)]
    /// [DEPRECATED] Update existing documentation comment - use 'rs-hack update' instead
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack update --name <NAME> --node-type <TYPE> --doc-comment <TEXT>' instead

MIGRATION:
    Old: rs-hack update-doc-comment --target-type struct --name User --doc-comment \"Updated user model\"
    New: rs-hack update --name User --node-type struct --doc-comment \"Updated user model\" --paths src --apply")]
    UpdateDocComment {
        /// Path to Rust file(s) - supports multiple paths and glob patterns
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Type of target: "struct", "enum", "function", "field", "variant"
        #[arg(short = 't', long)]
        target_type: String,

        /// Name of the target
        #[arg(short, long)]
        name: String,

        /// New documentation comment text
        #[arg(short = 'd', long)]
        doc_comment: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    #[command(hide = true)]
    /// [DEPRECATED] Remove documentation comment from an item - use 'rs-hack remove' instead
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack remove --name <NAME> --node-type <TYPE> --doc-comment' instead

MIGRATION:
    Old: rs-hack remove-doc-comment --target-type struct --name User
    New: rs-hack remove --name User --node-type struct --doc-comment --paths src --apply")]
    RemoveDocComment {
        /// Path to Rust file(s) - supports multiple paths and glob patterns
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Type of target: "struct", "enum", "function", "field", "variant"
        #[arg(short = 't', long)]
        target_type: String,

        /// Name of the target
        #[arg(short, long)]
        name: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    #[command(hide = true)]
    /// [DEPRECATED] Find all occurrences of a field across the codebase - use 'rs-hack find' instead
    #[command(after_help = "⚠️  DEPRECATED: Use 'rs-hack find --field-name <FIELD>' instead

MIGRATION:
    Old: rs-hack find-field --field-name color
    New: rs-hack find --field-name color --paths src

EXAMPLES:
    # Find all occurrences of a field
    rs-hack find-field --paths src --field-name immediate_mode

    # Show summary only (don't list all literal occurrences)
    rs-hack find-field --paths src --field-name debug_mode --summary

WHAT IT DOES:
    This command searches for a field in three places:
    1. Struct definitions (where the field is declared)
    2. Enum variant definitions (for enum variants with fields)
    3. Struct literal expressions (where the field is initialized)

    It provides suggested commands for removing the field from each location.")]
    FindField {
        /// Path to Rust file(s) - supports glob patterns
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the field to search for
        #[arg(short = 'n', long)]
        field_name: String,

        /// Show summary only (don't list all literal occurrences)
        #[arg(long)]
        summary: bool,
    },
}

#[derive(Subcommand)]
enum ArchCommands {
    /// Extract annotations and build the graph
    Extract {
        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Output format (json, mermaid)
        #[arg(short, long, default_value = "json")]
        format: String,

        /// Show verbose extraction progress
        #[arg(short, long)]
        verbose: bool,
    },

    /// Query the architecture graph
    Query {
        /// Query string (e.g., "layer:core AND role:compiler")
        query: String,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Output format (ids, json, verbose)
        #[arg(short, long, default_value = "ids")]
        format: String,
    },

    /// Trace paths between nodes
    Trace {
        /// Source query
        from: String,

        /// Target query
        to: String,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },

    /// Get architectural context for a file
    Context {
        /// File path to get context for
        file: PathBuf,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Output format (markdown, json)
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },

    /// Validate architecture rules
    Validate {
        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Path to rules TOML file (default: load from Cargo.toml metadata)
        #[arg(short, long)]
        rules: Option<PathBuf>,

        /// Only show warnings and errors
        #[arg(short, long)]
        quiet: bool,

        /// Also validate layer dependencies from schema
        #[arg(long)]
        include_schema_rules: bool,
    },

    /// Export graph visualization
    Visualize {
        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Output format (mermaid, dot)
        #[arg(short, long, default_value = "mermaid")]
        format: String,
    },

    /// Print the schema loaded from Cargo.toml metadata
    Schema {
        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Output format (summary, toml, json)
        #[arg(short, long, default_value = "summary")]
        format: String,
    },

    /// Initialize arch metadata in Cargo.toml
    Init {
        /// Output format (toml, example)
        #[arg(short, long, default_value = "toml")]
        format: String,
        /// Write to Cargo.toml instead of printing
        #[arg(long)]
        apply: bool,
        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },

}

/// Subcommands for `rs-hack board` — the hack-board kanban surface.
///
/// Everything here is board-facing: running the UI, installing slash
/// commands, listing tickets, claiming IDs, writing summaries, and
/// surfacing the SDLC ruleset. Kept out of the top-level namespace so
/// that `rs-hack --help` stays focused on refactoring.
#[derive(Subcommand)]
enum BoardCommands {
    /// Start the hack-board kanban server
    Serve {
        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// HTTP port (default: auto-picked from workspace hash in [3333, 3998])
        #[arg(long)]
        port: Option<u16>,

        /// UDP nudge port (default: HTTP port + 1)
        #[arg(long)]
        udp_port: Option<u16>,

        /// Open browser automatically
        #[arg(long)]
        open: bool,
    },

    /// Install hack-board slash commands and CLAUDE.md snippet into a project
    ///
    /// Writes .claude/commands/{comment,handoff,refine}.md and appends a
    /// hack-board section to CLAUDE.md (or creates it). Idempotent.
    Init {
        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Overwrite existing files instead of skipping them
        #[arg(long)]
        force: bool,
    },

    /// Show a single ticket or relay by ID
    ///
    /// Default output is a compact human view: header, status line,
    /// source, gotchas, handoff (completed), next steps, verify,
    /// assumes, cleanup, see-also, child IDs (for parents/epics), and
    /// any duplicate-ID conflicts. Use `--prompt` for the full
    /// continuation-prompt form an agent would consume on pickup
    /// (equivalent to `rs-hack board tickets --prompt <ID>`). Use
    /// `--format json` for the raw Ticket struct.
    ///
    /// Compound IDs like `R022-T1` are looked up directly.
    Show {
        /// Ticket or relay ID (e.g. R022, R022-T1)
        id: String,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Emit the full agent-pickup continuation prompt instead of
        /// the compact view (same as `tickets --prompt <ID>`)
        #[arg(long)]
        prompt: bool,

        /// Output format: markdown (default) | json
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },

    /// Print a slash-command prompt template (refine | handoff | comment)
    ///
    /// Use this when the slash commands aren't installed in your harness
    /// (i.e. `.claude/commands/{name}.md` is missing because `board init`
    /// hasn't been run, or you're in a harness that doesn't read those
    /// files). The output is the same prompt content `/refine` /
    /// `/handoff` / `/comment` would inject — paste it as your next
    /// instruction or follow it directly. With no NAME, lists available
    /// prompts.
    Prompt {
        /// Prompt name: refine | handoff | comment (omit to list)
        name: Option<String>,
    },

    /// List @yah:ticket and @yah:relay work items from source
    Tickets {
        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Output format (markdown, json)
        #[arg(short, long, default_value = "markdown")]
        format: String,

        /// Filter by status (open, claimed, in-progress, handoff, review, done)
        #[arg(short, long)]
        status: Option<String>,

        /// Filter by assignee (e.g., agent:claude)
        #[arg(short, long)]
        assignee: Option<String>,

        /// Only include epics (relays with @yah:kind(epic) or inferred children)
        #[arg(long)]
        epics: bool,

        /// Generate continuation prompt for a specific ticket ID (e.g., R001)
        #[arg(long)]
        prompt: Option<String>,

        /// Generate relay doc for a specific ticket ID
        #[arg(long)]
        relay_doc: Option<String>,
    },

    /// Take ownership of work — either by creating a new ticket/relay in the
    /// **Active** column, or by flipping an existing Open ticket into Active.
    ///
    /// Two forms:
    ///
    ///   rs-hack board claim --kind <K> --file <F> --title <T>   → create + claim
    ///   rs-hack board claim <ID>                                → claim existing
    ///
    /// Creating: scans source for the next free ID under a file lock so two
    /// agents running concurrently can't collide, writes the annotation block
    /// at the top of `--file`, and sets assignee to the current agent.
    ///
    /// Flipping: finds the ticket with ID `<ID>`, rewrites its `@yah:status`
    /// from `open` to `in-progress`, and sets assignee. Refuses if the ticket
    /// isn't currently Open.
    ///
    /// Prints the claimed ID to stdout.
    Claim {
        /// Existing ticket/relay ID to claim (e.g. R003). Mutually exclusive
        /// with `--kind` / `--file` / `--title`.
        #[arg(
            conflicts_with_all = ["kind", "file", "title"],
        )]
        id: Option<String>,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Kind of item to create: relay | feature | bug | task | epic
        #[arg(short, long, required_unless_present = "id")]
        kind: Option<String>,

        /// Target source file the annotation will be written to
        #[arg(short, long, required_unless_present = "id")]
        file: Option<PathBuf>,

        /// Title (required when creating)
        #[arg(short, long, required_unless_present = "id")]
        title: Option<String>,

        /// @yah:assignee(...)
        #[arg(long)]
        assignee: Option<String>,

        /// @yah:status(...). Defaults to `in-progress` for tickets and `handoff` for relays.
        #[arg(long)]
        status: Option<String>,

        /// @yah:phase(...)
        #[arg(long)]
        phase: Option<String>,

        /// @yah:parent(...)
        #[arg(long)]
        parent: Option<String>,

        /// @yah:severity(...) (bug-specific)
        #[arg(long)]
        severity: Option<String>,

        /// @yah:handoff("...") — repeatable. Pass multiple times for a
        /// file-grouped / bullet-per-chunk handoff; single use renders as
        /// a paragraph.
        #[arg(long)]
        handoff: Vec<String>,

        /// @yah:next("...") — repeatable
        #[arg(long)]
        next: Vec<String>,

        /// @yah:verify("...") — repeatable
        #[arg(long)]
        verify: Vec<String>,

        /// @yah:cleanup("...") — repeatable
        #[arg(long)]
        cleanup: Vec<String>,

        /// @yah:gotcha("...") — repeatable. Pre-existing breakage / traps the
        /// next agent needs to know up front. Rendered above the context block
        /// in the pickup prompt.
        #[arg(long)]
        gotcha: Vec<String>,

        /// @yah:assumes("...") — repeatable. Flags an unverified claim that
        /// was baked into the handoff. Rendered as a risks section in the
        /// pickup prompt so the next agent knows to confirm or challenge.
        #[arg(long)]
        assumes: Vec<String>,

        /// @arch:see(...) — repeatable
        #[arg(long)]
        see: Vec<String>,

        /// Emit machine-readable JSON `{id, file, line}` instead of just the ID
        #[arg(long)]
        json: bool,
    },

    /// Declare a new unclaimed ticket / relay in the **Open** column.
    ///
    /// This is the "file an issue" verb — it creates the ID and annotation but
    /// leaves the work unassigned. No assignee, no handoff payload. When an
    /// agent is ready to take it on, they run `rs-hack board claim <ID>` which
    /// flips it into Active.
    ///
    /// Accepts the same ID/placement flags as `claim` (kind/file/title/phase/
    /// parent/severity) plus structural payload that survives claim
    /// (next/verify/cleanup/see). Rejects `--assignee`, `--handoff`,
    /// `--gotcha`, `--assumes` — those belong to a handoff, not an inbox item.
    Open {
        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Kind of item to create: relay | feature | bug | task | epic
        #[arg(short, long)]
        kind: String,

        /// Target source file the annotation will be written to
        #[arg(short, long)]
        file: PathBuf,

        /// Title (required)
        #[arg(short, long)]
        title: String,

        /// @yah:phase(...)
        #[arg(long)]
        phase: Option<String>,

        /// @yah:parent(...)
        #[arg(long)]
        parent: Option<String>,

        /// @yah:severity(...) (bug-specific)
        #[arg(long)]
        severity: Option<String>,

        /// @yah:next("...") — repeatable
        #[arg(long)]
        next: Vec<String>,

        /// @yah:verify("...") — repeatable
        #[arg(long)]
        verify: Vec<String>,

        /// @yah:cleanup("...") — repeatable
        #[arg(long)]
        cleanup: Vec<String>,

        /// @arch:see(...) — repeatable
        #[arg(long)]
        see: Vec<String>,

        /// Emit machine-readable JSON `{id, file, line}` instead of just the ID
        #[arg(long)]
        json: bool,
    },

    /// Transition an existing ticket to a target column.
    ///
    /// Mirrors the drag-and-drop transitions the board UI enforces:
    ///   open → active
    ///   active → open | handoff | review
    ///   handoff → active | review
    ///   review → handoff
    ///
    /// Carries the same payload flags as `claim` — `--handoff`, `--next`,
    /// `--verify`, `--gotcha`, `--assumes`, `--cleanup` — and appends them
    /// to the ticket's existing annotation block. Use this instead of
    /// creating a new annotation when finishing a phase (Rule03: update the same
    /// relay in place).
    Move {
        /// Ticket or relay ID (e.g. R003, F02, R007-T1)
        id: String,

        /// Target column: open | active | handoff | review
        column: String,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Set/overwrite @yah:assignee(...)
        #[arg(long)]
        assignee: Option<String>,

        /// @yah:handoff("...") — repeatable. Appended to the existing block.
        #[arg(long)]
        handoff: Vec<String>,

        /// @yah:next("...") — repeatable
        #[arg(long)]
        next: Vec<String>,

        /// @yah:verify("...") — repeatable
        #[arg(long)]
        verify: Vec<String>,

        /// @yah:cleanup("...") — repeatable
        #[arg(long)]
        cleanup: Vec<String>,

        /// @yah:gotcha("...") — repeatable
        #[arg(long)]
        gotcha: Vec<String>,

        /// @yah:assumes("...") — repeatable
        #[arg(long)]
        assumes: Vec<String>,
    },

    /// Archive a ticket — strips its `@yah:` annotations from source and
    /// appends an `archived` event to `.yah/events/<shard>.jsonl`.
    ///
    /// Allowed only when the ticket is in `review`, `done`, `handoff`, or
    /// `open`. Refuses `claimed` / `in-progress` (those must transition out
    /// of active first). The snapshot is preserved in the shard, so an
    /// archived ticket is still inspectable via `rs-hack board show <ID>`
    /// and unarchive-able by re-pasting the annotation block.
    ///
    /// This is the agent-facing path — equivalent to clicking the archive
    /// button on the kanban UI, but works without the server running.
    Archive {
        /// Ticket or relay ID (e.g. R003, R007-T1)
        id: String,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },

    /// Write a freeform summary to the hack-board inbox
    Summary {
        /// Summary text (markdown). Use - to read from stdin.
        text: String,

        /// Link to a ticket ID (e.g., F003)
        #[arg(short, long)]
        ticket: Option<String>,

        /// Author (e.g., agent:claude)
        #[arg(short, long)]
        author: Option<String>,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },

    /// Attach a design comment to a relay (or to the inbox).
    ///
    /// Parallel to `summary`, but for design rationale rather than progress.
    /// Used by /refine and /design to record "why we picked this approach"
    /// without conflating it with the inbox-promotable summary stream.
    /// Stored in `.yah/comments/{relay}/` (or `.yah/comments/_inbox/`).
    Comment {
        /// Comment text (markdown). Use - to read from stdin.
        text: String,

        /// Relay (or compound ticket) the comment is attached to. If omitted,
        /// the comment lands in `.yah/comments/_inbox/`.
        #[arg(short, long)]
        relay: Option<String>,

        /// Author (e.g., agent:claude)
        #[arg(short, long)]
        author: Option<String>,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },

    /// Promote an inbox summary to a real `@yah:relay(...)` annotation.
    ///
    /// Allocates the next R-number under the workspace ID lock, writes the
    /// relay block to `--file` (must be a `.rs` file), and updates the
    /// summary's frontmatter to `promoted: true` with `relay_id` and
    /// `relay_title`. The relay lands in the Handoff column with the
    /// summary's first paragraph as `@yah:handoff(...)`.
    ///
    /// Prints the new relay ID to stdout (or `{id, file, line}` JSON with `--json`).
    Promote {
        /// Summary ID — the filename stem under `.yah/summaries/`
        /// (e.g. `summary-1776192331` or `R007-1776200000`).
        #[arg(long = "summary-id", visible_alias = "id")]
        summary_id: String,

        /// Target `.rs` file the relay annotation will be written to.
        #[arg(short, long)]
        file: PathBuf,

        /// Optional title override. Defaults to the first non-blank line of
        /// the summary, truncated to 80 chars.
        #[arg(short, long)]
        title: Option<String>,

        /// `@yah:assignee(...)` value (e.g. `agent:claude`).
        #[arg(long)]
        assignee: Option<String>,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Emit machine-readable JSON `{relay_id, file, line}` instead of just the ID.
        #[arg(long)]
        json: bool,
    },

    /// Print the hack-board SDLC ruleset (Rule01-Rule12 + Col01 column map).
    ///
    /// The same rules are embedded in every continuation prompt
    /// (`rs-hack board tickets --prompt <ID>`). Running this command directly
    /// is the quickest way for an agent to orient itself on how work flows
    /// through the board.
    Rules {
        /// Narrow the ruleset to a specific situation:
        /// pickup | finishing | new-work | archive | refactor
        #[arg(short, long)]
        context: Option<String>,

        /// Output format: markdown (default) | json | terse (one-line rules, no why/apply)
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },

    /// Scannable "what's already in flight" view for planning agents (Rule10).
    ///
    /// Lists every Open / Active / Handoff relay and ticket, one line per
    /// item — `ID · assignee · title → arch-doc-ref`. Review, Done, and
    /// archived items are excluded (not in flight). Epics are excluded by
    /// default — they coordinate relays, not carry work (Rule06).
    ///
    /// Run this before `/refine` or `rs-hack board open --kind relay` so
    /// you notice when someone is already working on the problem you were
    /// about to plan. If you see overlap: claim the existing relay, add
    /// your plan as a sub-ticket under it, or reference it in your own arch
    /// doc.
    Inflight {
        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Output format: markdown (default) | json | grep (one line per item)
        #[arg(short, long, default_value = "markdown")]
        format: String,

        /// Include epics in the output (off by default — epics don't carry work)
        #[arg(long)]
        include_epics: bool,
    },

    /// One-shot summary of everything in flight — for planning agents.
    ///
    /// Counts per column, who's actively holding what (Rule05 off-limits), the
    /// handoff queue with one-line next steps, epic child rollups, pending
    /// todos, and smell from the event log (disappeared tickets). Different
    /// shape from `board tickets` — that's a per-ticket dump; this is a
    /// snapshot for orientation.
    Status {
        /// Optional ticket/relay ID — when given, dispatches to
        /// `rs-hack board show <ID>` (whole-board status if omitted)
        id: Option<String>,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Output format: markdown (default) | json
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },

    /// Manage the sibling-workspace registry consulted by `board claim`
    /// for cross-clone / cross-machine ID anti-collision (R002 P1).
    ///
    /// Same-machine `git worktree add` subtrees are auto-discovered from
    /// `git worktree list --porcelain` and don't need a registry entry.
    /// Use this command for separate clones (`local`) and SSH peers
    /// (`remote`). Stored at `.yah/worktrees.json` (gitignored — every
    /// clone owns its own view of the network).
    #[command(subcommand)]
    Worktrees(WorktreesCommands),
}

/// Sibling-workspace registry management.
#[derive(Subcommand)]
enum WorktreesCommands {
    /// Add a sibling. Detects `host:path` form for remote, otherwise
    /// treated as a local path. Idempotent.
    Add {
        /// Local path or `user@host:path` for SSH.
        target: String,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },

    /// List declared and auto-discovered siblings.
    List {
        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Output format: text (default) | json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Remove a sibling by path or `host:path`. Auto-discovered git
    /// worktrees can't be removed via this command — drop the worktree
    /// with `git worktree remove` instead.
    Remove {
        /// Local path or `host:path` to remove.
        target: String,

        /// Path to workspace root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },
}

/// Validate that an enum variant rename would catch all references
fn validate_enum_variant_rename(
    files: &[PathBuf],
    enum_name: &str,
    old_variant: &str,
    enum_path: Option<&str>,
) -> Result<()> {
    use syn::{visit::Visit, File};

    struct VariantFinder<'a> {
        enum_name: &'a str,
        variant_name: &'a str,
        enum_path: Option<&'a str>,
        references: Vec<(String, usize, usize, String)>, // (file, line, col, code)
    }

    impl<'a, 'ast> Visit<'ast> for VariantFinder<'a> {
        fn visit_path(&mut self, path: &'ast syn::Path) {
            let path_str = quote::quote!(#path).to_string();

            // Check if this path contains our variant
            // Handle cases like:
            // - EnumName::VariantName
            // - super::EnumName::VariantName
            // - crate::module::EnumName::VariantName
            if path_str.contains(self.variant_name) {
                let segments: Vec<_> = path.segments.iter().collect();
                let len = segments.len();

                if len >= 2 {
                    let enum_seg = &segments[len - 2];
                    let variant_seg = &segments[len - 1];

                    if enum_seg.ident == self.enum_name && variant_seg.ident == self.variant_name {
                        // Found a match - we'll record it in visit_file
                        syn::visit::visit_path(self, path);
                        return;
                    }
                } else if len == 1 && segments[0].ident == self.variant_name {
                    // Might be an imported variant
                    syn::visit::visit_path(self, path);
                    return;
                }
            }

            syn::visit::visit_path(self, path);
        }
    }

    let mut finder = VariantFinder {
        enum_name,
        variant_name: old_variant,
        enum_path,
        references: Vec::new(),
    };

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;

        let syntax_tree: File = syn::parse_str(&content)
            .with_context(|| format!("Failed to parse {}", file_path.display()))?;

        // Search for simple text matches (this catches things AST might miss)
        for (line_num, line) in content.lines().enumerate() {
            if line.contains(old_variant) {
                // Check if it's part of our enum variant pattern
                if line.contains(&format!("{}::{}", enum_name, old_variant)) ||
                   line.contains(&format!("::{}", old_variant)) {
                    finder.references.push((
                        file_path.display().to_string(),
                        line_num + 1,
                        0,
                        line.trim().to_string(),
                    ));
                }
            }
        }
    }

    if finder.references.is_empty() {
        println!("✓ No references to '{}::{}' found.", enum_name, old_variant);
        println!("  All occurrences have been renamed or there were none to begin with.");
    } else {
        println!("❌ Found {} remaining references to '{}::{}':",
                 finder.references.len(), enum_name, old_variant);
        println!();

        for (file, line, _col, code) in &finder.references {
            println!("  - {}:{}", file, line);
            println!("    {}", code);
        }

        println!();
        println!("💡 Suggestions:");
        if enum_path.is_none() {
            println!("  - Try using --enum-path to enable better matching of fully qualified paths");
        }
        println!("  - Run without --validate to rename these references");
        println!("  - Check if these are false positives (comments, strings, etc.)");
    }

    Ok(())
}

/// Validate that a function rename would catch all references
fn validate_function_rename(
    files: &[PathBuf],
    old_name: &str,
    function_path: Option<&str>,
) -> Result<()> {
    let mut references = Vec::new();

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;

        // Search for simple text matches
        for (line_num, line) in content.lines().enumerate() {
            if line.contains(old_name) {
                // Check if it looks like a function reference
                // This is a simple heuristic - matches function calls, definitions, etc.
                let patterns = [
                    format!("fn {}(", old_name),
                    format!("fn {}<", old_name),
                    format!("{}(", old_name),
                    format!("{}::", old_name),
                    format!("::{}", old_name),
                ];

                if patterns.iter().any(|p| line.contains(p)) {
                    references.push((
                        file_path.display().to_string(),
                        line_num + 1,
                        line.trim().to_string(),
                    ));
                }
            }
        }
    }

    if references.is_empty() {
        println!("✓ No references to '{}' found.", old_name);
        println!("  All occurrences have been renamed or there were none to begin with.");
    } else {
        println!("❌ Found {} remaining references to '{}':", references.len(), old_name);
        println!();

        for (file, line, code) in &references {
            println!("  - {}:{}", file, line);
            println!("    {}", code);
        }

        println!();
        println!("💡 Suggestions:");
        if function_path.is_none() {
            println!("  - Try using --function-path to enable better matching of fully qualified paths");
        }
        println!("  - Run without --validate to rename these references");
        println!("  - Check if these are false positives (comments, strings, etc.)");
    }

    Ok(())
}

/// Check if a target exists in the files
fn target_exists(files: &[PathBuf], name: &str, node_type: Option<&str>) -> Result<bool> {
    for file in files {
        let content = std::fs::read_to_string(file)
            .context(format!("Failed to read file: {:?}", file))?;

        let editor = match RustEditor::new(&content) {
            Ok(e) => e,
            Err(_) => continue, // Skip unparseable files during discovery
        };
        let results = editor.inspect(node_type, Some(name), None, false)?;

        if !results.is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Detect the type of a target (struct or enum) for derive operations
fn detect_target_type(files: &[PathBuf], name: &str) -> Result<Option<String>> {
    for file in files {
        let content = std::fs::read_to_string(file)
            .context(format!("Failed to read file: {:?}", file))?;

        let editor = match RustEditor::new(&content) {
            Ok(e) => e,
            Err(_) => continue, // Skip unparseable files during discovery
        };

        // Try struct first
        let struct_results = editor.inspect(Some("struct"), Some(name), None, false)?;
        if !struct_results.is_empty() {
            return Ok(Some("struct".to_string()));
        }

        // Try enum
        let enum_results = editor.inspect(Some("enum"), Some(name), None, false)?;
        if !enum_results.is_empty() {
            return Ok(Some("enum".to_string()));
        }
    }
    Ok(None)
}

/// Show helpful hints when target is not found
fn show_target_hints(files: &[PathBuf], name: &str, expected_type: &str, paths: &[PathBuf]) -> Result<()> {
    use std::collections::HashMap;
    use operations::InspectResult;

    // Search across all node types to find near-misses
    let mut hint_results: Vec<InspectResult> = Vec::new();

    for file in files {
        let content = std::fs::read_to_string(file)
            .context(format!("Failed to read file: {:?}", file))?;

        let editor = match RustEditor::new(&content) {
            Ok(e) => e,
            Err(_) => continue, // Skip unparseable files during discovery
        };
        let mut results = editor.inspect(None, Some(name), None, false)?;

        for result in &mut results {
            result.file_path = file.to_string_lossy().to_string();
        }

        hint_results.extend(results);
    }

    if hint_results.is_empty() {
        eprintln!("No {} found named \"{}\"", expected_type, name);
        eprintln!();
        eprintln!("Hint: Run 'find' to discover what exists:");
        eprintln!("  rs-hack find --paths {} --name {}",
            paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join(" "),
            name
        );
    } else {
        // Group by node type
        let mut by_type: HashMap<String, Vec<&InspectResult>> = HashMap::new();
        for result in &hint_results {
            by_type.entry(result.node_type.clone()).or_insert_with(Vec::new).push(result);
        }

        eprintln!("No {} found named \"{}\"", expected_type, name);
        eprintln!();
        eprintln!("Hint: Found \"{}\" in other contexts:", name);

        for (ntype, results) in by_type.iter() {
            let count = results.len();
            let first = results.first().unwrap();
            eprintln!("  - {} ({}): {}:{}:{}",
                ntype,
                if count == 1 { "1 match".to_string() } else { format!("{} matches", count) },
                first.file_path,
                first.location.line,
                first.location.column
            );
        }

        eprintln!();
        eprintln!("To see all matches, run:");
        eprintln!("  rs-hack find --paths {} --name {}",
            paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join(" "),
            name
        );
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Hack(hack_cmd) => match hack_cmd {
        HackCommands::AddStructField { paths, struct_name, field, position, literal_default, output, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::AddStructField(AddStructFieldOp {
                struct_name: struct_name.clone(),
                field_def: field.clone(),
                position: parse_position(&position)?,
                literal_default: literal_default.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::UpdateStructField { paths, struct_name, field, output, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::UpdateStructField(UpdateStructFieldOp {
                struct_name: struct_name.clone(),
                field_def: field.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::RemoveStructField { paths, struct_name, field_name, literal_only, output, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::RemoveStructField(RemoveStructFieldOp {
                struct_name: struct_name.clone(),
                field_name: field_name.clone(),
                literal_only,
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::AddStructLiteralField { paths, struct_name, field, position, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::AddStructLiteralField(AddStructLiteralFieldOp {
                struct_name: struct_name.clone(),
                field_def: field.clone(),
                position: parse_position(&position)?,
                struct_path: None,  // Deprecated command doesn't support path resolution
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::AddEnumVariant { paths, enum_name, variant, position, output, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::AddEnumVariant(AddEnumVariantOp {
                enum_name: enum_name.clone(),
                variant_def: variant.clone(),
                position: parse_position(&position)?,
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::UpdateEnumVariant { paths, enum_name, variant, output, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::UpdateEnumVariant(UpdateEnumVariantOp {
                enum_name: enum_name.clone(),
                variant_def: variant.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::RemoveEnumVariant { paths, enum_name, variant_name, output, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::RemoveEnumVariant(RemoveEnumVariantOp {
                enum_name: enum_name.clone(),
                variant_name: variant_name.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::RenameEnumVariant { paths, enum_name, old_variant, new_variant, enum_path, edit_mode, validate, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            // If validate mode, run validation instead of rename
            if validate {
                validate_enum_variant_rename(&files, &enum_name, &old_variant, enum_path.as_deref())?;
            } else {
                // Parse edit mode
                let edit_mode = edit_mode.parse::<EditMode>()
                    .map_err(|e| anyhow::anyhow!("{}", e))?;

                let op = Operation::RenameEnumVariant(RenameEnumVariantOp {
                    enum_name: enum_name.clone(),
                    old_variant: old_variant.clone(),
                    new_variant: new_variant.clone(),
                    enum_path: enum_path.clone(),
                    edit_mode,
                });

                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
            }
        }

        HackCommands::RenameFunction { paths, old_name, new_name, function_path, edit_mode, validate, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            // If validate mode, run validation instead of rename
            if validate {
                validate_function_rename(&files, &old_name, function_path.as_deref())?;
            } else {
                // Parse edit mode
                let edit_mode = edit_mode.parse::<EditMode>()
                    .map_err(|e| anyhow::anyhow!("{}", e))?;

                let op = Operation::RenameFunction(RenameFunctionOp {
                    old_name: old_name.clone(),
                    new_name: new_name.clone(),
                    function_path: function_path.clone(),
                    edit_mode,
                });

                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
            }
        }

        HackCommands::Rename { paths, name, to, enum_path, function_path, kind, node_type, edit_mode, validate, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            // Parse edit mode
            let edit_mode = edit_mode.parse::<EditMode>()
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            // Handle granular renaming with --node-type (for expression-level nodes)
            // For these, delegate to Transform with Replace action
            let granular_types = ["function-call", "method-call", "identifier", "macro-call", "struct-literal", "enum-usage", "type-ref"];

            if let Some(nt) = &node_type {
                if granular_types.contains(&nt.as_str()) {
                    // Use Transform operation for granular renaming
                    let op = Operation::Transform(TransformOp {
                        node_type: nt.clone(),
                        name_filter: Some(name.clone()),
                        content_filter: None,
                        action: TransformAction::Replace { with: to.clone() },
                    });
                    execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                    return Ok(());
                }
            }

            // Handle --kind expansion for semantic grouping
            if let Some(k) = &kind {
                let expanded = expand_kind_to_node_types(k);
                if expanded.is_empty() {
                    anyhow::bail!("Unknown kind '{}'. Valid kinds: struct, function, enum, match, identifier, type, macro, const, trait, mod, use", k);
                }

                // For "function" kind, rename both definition and all calls
                if k == "function" {
                    // First rename the function definition (falls through to normal logic below)
                    // The normal rename logic will handle both definition and usages
                } else if k == "identifier" {
                    // For identifier kind, use Transform
                    let op = Operation::Transform(TransformOp {
                        node_type: "identifier".to_string(),
                        name_filter: Some(name.clone()),
                        content_filter: None,
                        action: TransformAction::Replace { with: to.clone() },
                    });
                    execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                    return Ok(());
                } else {
                    anyhow::bail!("Rename with --kind is only supported for 'function' and 'identifier' kinds. For other kinds, use --node-type.");
                }
            }

            // Auto-detect: Check if name contains :: for enum variant syntax
            if name.contains("::") {
                // Parse as enum variant (EnumName::VariantName)
                let parts: Vec<&str> = name.split("::").collect();
                if parts.len() != 2 {
                    anyhow::bail!("Invalid enum variant syntax. Use EnumName::VariantName");
                }

                let enum_name = parts[0];
                let old_variant = parts[1];

                // Check if the enum exists
                if !target_exists(&files, enum_name, Some("enum"))? {
                    show_target_hints(&files, enum_name, "enum", &paths)?;
                    return Ok(());
                }

                // If validate mode, run validation instead of rename
                if validate {
                    validate_enum_variant_rename(&files, enum_name, old_variant, enum_path.as_deref())?;
                } else {
                    let op = Operation::RenameEnumVariant(RenameEnumVariantOp {
                        enum_name: enum_name.to_string(),
                        old_variant: old_variant.to_string(),
                        new_variant: to.clone(),
                        enum_path: enum_path.clone(),
                        edit_mode,
                    });

                    execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                }
            } else {
                // No :: syntax - need to discover if it's a function or enum variant
                // First check if it exists as any kind of function (standalone, impl-method, trait-method)
                let is_function = target_exists(&files, &name, Some("function"))? ||
                                  target_exists(&files, &name, Some("impl-method"))? ||
                                  target_exists(&files, &name, Some("trait-method"))?;

                // Check if any enum has a variant with this name
                let mut found_as_enum_variant = false;
                let mut enum_candidates: Vec<String> = Vec::new();

                for file in &files {
                    let content = std::fs::read_to_string(file)
                        .context(format!("Failed to read file: {:?}", file))?;

                    let editor = match RustEditor::new(&content) {
                        Ok(e) => e,
                        Err(_) => continue, // Skip unparseable files during discovery
                    };
                    // Search for enums
                    let enum_results = editor.inspect(Some("enum"), None, None, false)?;

                    for enum_result in enum_results {
                        // Check if this enum has a variant matching our name
                        if enum_result.snippet.contains(&format!("{}(", &name)) ||
                           enum_result.snippet.contains(&format!("{} {{", &name)) ||
                           enum_result.snippet.contains(&format!("{},", &name)) {
                            found_as_enum_variant = true;
                            enum_candidates.push(enum_result.identifier.clone());
                        }
                    }
                }

                // Determine what to do based on what we found
                if is_function && found_as_enum_variant {
                    // Ambiguous - both exist
                    anyhow::bail!(
                        "Ambiguous target '{}': found both as a function and as an enum variant.\n\
                         Please disambiguate:\n\
                         - For function: rs-hack rename --name {} --to {} --paths ... --apply\n\
                         - For enum variant: rs-hack rename --name <EnumName>::{} --to {} --paths ... --apply\n\
                         \n\
                         Found in enums: {}",
                        name, name, to, name, to,
                        enum_candidates.join(", ")
                    );
                } else if is_function {
                    // Rename function
                    if validate {
                        validate_function_rename(&files, &name, function_path.as_deref())?;
                    } else {
                        let op = Operation::RenameFunction(RenameFunctionOp {
                            old_name: name.clone(),
                            new_name: to.clone(),
                            function_path: function_path.clone(),
                            edit_mode,
                        });

                        execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                    }
                } else if found_as_enum_variant {
                    // Found as enum variant, but need to know which enum
                    if enum_candidates.len() == 1 {
                        // Only one enum has this variant - can proceed
                        let enum_name = &enum_candidates[0];

                        if validate {
                            validate_enum_variant_rename(&files, enum_name, &name, enum_path.as_deref())?;
                        } else {
                            let op = Operation::RenameEnumVariant(RenameEnumVariantOp {
                                enum_name: enum_name.clone(),
                                old_variant: name.clone(),
                                new_variant: to.clone(),
                                enum_path: enum_path.clone(),
                                edit_mode,
                            });

                            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                        }
                    } else {
                        // Multiple enums have this variant
                        anyhow::bail!(
                            "Variant '{}' found in multiple enums: {}\n\
                             Please specify which enum using :: syntax:\n\
                             rs-hack rename --name <EnumName>::{} --to {} --paths ... --apply",
                            name, enum_candidates.join(", "), name, to
                        );
                    }
                } else {
                    // Not found as function or enum variant
                    eprintln!("No function or enum variant found named \"{}\"", name);
                    eprintln!();
                    eprintln!("Hint: Run 'find' to discover what exists:");
                    eprintln!("  rs-hack find --paths {} --name {}",
                        paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join(" "),
                        name
                    );
                }
            }
        }

        HackCommands::AddMatchArm { paths, pattern, body, function, auto_detect, enum_name, apply } => {
            // Validate auto_detect requires enum_name
            if auto_detect && enum_name.is_none() {
                anyhow::bail!("--enum-name is required when using --auto-detect");
            }

            // Validate pattern is provided when not using auto_detect
            if !auto_detect && pattern.is_none() {
                anyhow::bail!("--pattern is required when not using --auto-detect");
            }

            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::AddMatchArm(AddMatchArmOp {
                pattern: pattern.unwrap_or_default(),
                body: body.clone(),
                function_name: function,
                auto_detect,
                enum_name,
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::UpdateMatchArm { paths, pattern, body, function, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::UpdateMatchArm(UpdateMatchArmOp {
                pattern: pattern.clone(),
                new_body: body.clone(),
                function_name: function,
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::RemoveMatchArm { paths, pattern, function, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::RemoveMatchArm(RemoveMatchArmOp {
                pattern: pattern.clone(),
                function_name: function,
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::Batch { spec, apply } => {
            let content = std::fs::read_to_string(&spec)
                .context("Failed to read batch spec file")?;

            // Auto-detect format based on file extension
            let batch: BatchSpec = if spec.extension().and_then(|s| s.to_str()) == Some("yaml")
                || spec.extension().and_then(|s| s.to_str()) == Some("yml") {
                serde_yaml::from_str(&content)
                    .context("Failed to parse batch spec YAML")?
            } else {
                // Try JSON first, fall back to YAML if JSON fails
                serde_json::from_str(&content)
                    .or_else(|_| serde_yaml::from_str(&content))
                    .context("Failed to parse batch spec (tried both JSON and YAML)")?
            };

            execute_batch(&batch, apply, &cli.exclude)?;
        }
        
        HackCommands::Find { paths, kind, node_type, name, variant, content_filter, field_name, include_comments, format } => {
            use operations::InspectResult;

            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            // Expand kind to node_types if provided
            let node_types_to_search: Vec<Option<&str>> = if let Some(k) = &kind {
                let expanded = expand_kind_to_node_types(k);
                if expanded.is_empty() {
                    anyhow::bail!("Unknown kind '{}'. Valid kinds: struct, function, enum, match, identifier, type, macro, const, trait, mod, use", k);
                }
                expanded.into_iter().map(Some).collect()
            } else if let Some(ref nt) = node_type {
                vec![Some(nt.as_str())]
            } else {
                vec![None]
            };

            // Handle --field-name flag (field finding mode)
            if let Some(field) = field_name {
                use operations::{FieldLocation, FieldContext};
                use std::collections::HashMap;

                let mut all_locations: Vec<FieldLocation> = Vec::new();

                for file in &files {
                    let content = std::fs::read_to_string(&file)
                        .context(format!("Failed to read file: {:?}", file))?;

                    let editor = match RustEditor::new(&content) {
                        Ok(e) => e,
                        Err(e) => {
                            eprintln!("⚠️  Skipping {}: {}", file.display(), e);
                            continue;
                        }
                    };
                    let mut locations = editor.find_field_locations(&field)?;

                    // Fill in file paths
                    for location in &mut locations {
                        location.file_path = file.to_string_lossy().to_string();
                    }

                    all_locations.extend(locations);
                }

                if all_locations.is_empty() {
                    println!("No occurrences of field '{}' found.", field);
                    return Ok(());
                }

                // Group by context type
                let mut struct_defs: Vec<&FieldLocation> = Vec::new();
                let mut variant_defs: Vec<&FieldLocation> = Vec::new();
                let mut struct_literals: Vec<&FieldLocation> = Vec::new();

                for loc in &all_locations {
                    match &loc.context {
                        FieldContext::StructDefinition { .. } => struct_defs.push(loc),
                        FieldContext::EnumVariantDefinition { .. } => variant_defs.push(loc),
                        FieldContext::StructLiteral { .. } => struct_literals.push(loc),
                    }
                }

                // Display results
                println!("Found {} occurrence{} of field '{}':\n",
                    all_locations.len(),
                    if all_locations.len() == 1 { "" } else { "s" },
                    field);

                if !struct_defs.is_empty() {
                    println!("Struct Definitions ({}):", struct_defs.len());
                    for loc in &struct_defs {
                        if let FieldContext::StructDefinition { struct_name, field_type } = &loc.context {
                            println!("  - {}:{} in struct {} (type: {})",
                                loc.file_path, loc.line, struct_name, field_type);
                            println!("    Remove: rs-hack remove --name {} --field-name {} --paths {} --apply",
                                struct_name, field, loc.file_path);
                        }
                    }
                    println!();
                }

                if !variant_defs.is_empty() {
                    println!("Enum Variant Definitions ({}):", variant_defs.len());
                    for loc in &variant_defs {
                        if let FieldContext::EnumVariantDefinition { enum_name, variant_name, field_type } = &loc.context {
                            println!("  - {}:{} in enum {}::{} (type: {})",
                                loc.file_path, loc.line, enum_name, variant_name, field_type);
                            println!("    Remove: rs-hack remove --name {}::{} --field-name {} --paths {} --apply",
                                enum_name, variant_name, field, loc.file_path);
                        }
                    }
                    println!();
                }

                if !struct_literals.is_empty() {
                    println!("Struct Literal Expressions ({}):", struct_literals.len());
                    // Group by struct name for cleaner output
                    let mut by_struct: HashMap<String, Vec<&FieldLocation>> = HashMap::new();
                    for loc in &struct_literals {
                        if let FieldContext::StructLiteral { struct_name } = &loc.context {
                            by_struct.entry(struct_name.clone()).or_insert_with(Vec::new).push(loc);
                        }
                    }

                    let mut struct_names: Vec<String> = by_struct.keys().cloned().collect();
                    struct_names.sort();

                    for struct_name in struct_names {
                        let locs = &by_struct[&struct_name];
                        println!("  {} ({} occurrence{}):", struct_name, locs.len(),
                            if locs.len() == 1 { "" } else { "s" });
                        for loc in locs {
                            println!("    - {}:{}", loc.file_path, loc.line);
                        }
                        println!("    Remove from literals: rs-hack remove --name {} --field-name {} --literal-only --paths src --apply",
                            struct_name, field);
                    }
                    println!();
                }

                return Ok(());
            }

            let mut all_results: Vec<InspectResult> = Vec::new();

            for file in &files {
                let content = std::fs::read_to_string(&file)
                    .context(format!("Failed to read file: {:?}", file))?;

                let editor = match RustEditor::new(&content) {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("⚠️  Skipping {}: {}", file.display(), e);
                        continue;
                    }
                };

                // Search for all node types (if kind was provided, this will be multiple types)
                for node_type_to_search in &node_types_to_search {
                    let mut results = editor.inspect(
                        *node_type_to_search,
                        name.as_deref(),
                        variant.as_deref(),
                        include_comments
                    )?;

                    // Fill in file paths
                    for result in &mut results {
                        result.file_path = file.to_string_lossy().to_string();
                    }

                    // Apply content filter if specified
                    if let Some(ref filter) = content_filter {
                        results.retain(|r| r.snippet.contains(filter));
                    }

                    all_results.extend(results);
                }
            }

            // Hints system: If we found nothing with a specific node-type, check if other types have matches
            if all_results.is_empty() && node_type.is_some() && name.is_some() {
                // Re-run search across all node types to find near-misses
                let mut hint_results: Vec<InspectResult> = Vec::new();

                for file in &files {
                    let content = std::fs::read_to_string(&file)
                        .context(format!("Failed to read file: {:?}", file))?;

                    let editor = match RustEditor::new(&content) {
                        Ok(e) => e,
                        Err(_) => continue,
                    };
                    let mut results = editor.inspect(
                        None,  // Search all types
                        name.as_deref(),
                        variant.as_deref(),
                        false  // No comments needed for hints
                    )?;

                    for result in &mut results {
                        result.file_path = file.to_string_lossy().to_string();
                    }

                    if let Some(ref filter) = content_filter {
                        results.retain(|r| r.snippet.contains(filter));
                    }

                    hint_results.extend(results);
                }

                // If we found matches in other node types, show hints
                if !hint_results.is_empty() {
                    use std::collections::HashMap;

                    // Group by node type
                    let mut by_type: HashMap<String, Vec<&InspectResult>> = HashMap::new();
                    for result in &hint_results {
                        by_type.entry(result.node_type.clone()).or_insert_with(Vec::new).push(result);
                    }

                    // Display helpful message
                    eprintln!("No {} found named \"{}\"",
                        node_type.as_ref().unwrap(),
                        name.as_ref().unwrap());
                    eprintln!();
                    eprintln!("Hint: Found \"{}\" in other contexts:", name.as_ref().unwrap());

                    for (ntype, results) in by_type.iter() {
                        let count = results.len();
                        let first = results.first().unwrap();
                        eprintln!("  - {} ({}): {}:{}:{}",
                            ntype,
                            if count == 1 { "1 match".to_string() } else { format!("{} matches", count) },
                            first.file_path,
                            first.location.line,
                            first.location.column
                        );
                    }

                    eprintln!();
                    eprintln!("To see all matches, run without --node-type:");
                    eprintln!("  rs-hack find --paths {} --name {}",
                        paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join(" "),
                        name.as_ref().unwrap()
                    );

                    return Ok(());
                }
            }

            // Fallback: If we still found nothing with a name filter, do a text search
            if all_results.is_empty() && name.is_some() {
                let search_name = name.as_ref().unwrap();
                let mut text_matches: Vec<(String, usize)> = Vec::new();

                for file in &files {
                    let content = std::fs::read_to_string(&file)
                        .context(format!("Failed to read file: {:?}", file))?;

                    let count = content.lines().filter(|line| line.contains(search_name)).count();
                    if count > 0 {
                        text_matches.push((file.to_string_lossy().to_string(), count));
                    }
                }

                if !text_matches.is_empty() {
                    let total_matches: usize = text_matches.iter().map(|(_, c)| c).sum();

                    eprintln!("No AST nodes found for \"{}\"", search_name);
                    eprintln!();
                    eprintln!("However, found {} non-AST text occurrence{} of \"{}\":",
                        total_matches,
                        if total_matches == 1 { "" } else { "s" },
                        search_name
                    );

                    for (file_path, count) in &text_matches {
                        eprintln!("  - {} ({} line{})",
                            file_path,
                            count,
                            if *count == 1 { "" } else { "s" }
                        );
                    }

                    eprintln!();
                    eprintln!("Note: These occurrences may be:");
                    eprintln!("  - Inside macro invocations (e.g., vec![YourStruct {{ ... }}])");
                    eprintln!("  - In comments or strings");
                    eprintln!("  - Part of a qualified path (e.g., module::{})", search_name);
                    eprintln!();
                    eprintln!("rs-hack's AST visitor cannot see inside macro expansions.");
                    eprintln!("Try searching without --name to see all struct literals,");
                    eprintln!("or use --name with a different pattern (e.g., \"*::{}\").", search_name);

                    return Ok(());
                }
            }

            // Format output based on format flag
            match format.as_str() {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&all_results)?);
                }
                "locations" => {
                    for result in &all_results {
                        println!("{}:{}:{}", result.file_path, result.location.line, result.location.column);
                    }
                }
                "snippets" => {
                    // If searching all types (node_type is None), group results by type
                    if node_type.is_none() && !all_results.is_empty() {
                        use std::collections::HashMap;

                        // Group by node type
                        let mut by_type: HashMap<String, Vec<&InspectResult>> = HashMap::new();
                        for result in &all_results {
                            by_type.entry(result.node_type.clone()).or_insert_with(Vec::new).push(result);
                        }

                        // Display header
                        if let Some(ref search_name) = name {
                            println!("Found \"{}\" in {} context{}:\n",
                                search_name,
                                by_type.len(),
                                if by_type.len() == 1 { "" } else { "s" }
                            );
                        } else {
                            println!("Found {} result{} across {} node type{}:\n",
                                all_results.len(),
                                if all_results.len() == 1 { "" } else { "s" },
                                by_type.len(),
                                if by_type.len() == 1 { "" } else { "s" }
                            );
                        }

                        // Sort node types for consistent output
                        let mut type_names: Vec<String> = by_type.keys().cloned().collect();
                        type_names.sort();

                        // Display each group
                        for type_name in type_names {
                            let results = &by_type[&type_name];
                            let count = results.len();

                            println!("{}{}{}:",
                                type_name,
                                if count > 1 { format!(" ({} match{})", count, if count == 1 { "" } else { "es" }) } else { String::new() },
                                ""
                            );

                            for result in results {
                                println!("  // {}:{}:{} - {}",
                                    result.file_path,
                                    result.location.line,
                                    result.location.column,
                                    result.identifier);
                                // Show preceding comment if present
                                if let Some(ref comment) = result.preceding_comment {
                                    // Indent comment
                                    for line in comment.lines() {
                                        println!("  {}", line);
                                    }
                                }
                                // Indent snippet
                                for line in result.snippet.lines() {
                                    println!("  {}", line);
                                }
                                println!();
                            }
                        }

                        // Add hints for struct-literal searches with simple names
                        if let Some(ref search_name) = name {
                            if !search_name.contains("::") &&
                               (node_type.as_deref() == Some("struct-literal") ||
                                kind.as_deref() == Some("struct")) {
                                // Check if we found struct literals with qualified paths
                                if let Some(struct_lit_results) = by_type.get("struct-literal") {
                                    use std::collections::HashMap;
                                    let mut qualified_paths: HashMap<String, usize> = HashMap::new();

                                    for result in struct_lit_results {
                                        // Check if identifier contains :: (qualified path)
                                        if result.identifier.contains("::") {
                                            *qualified_paths.entry(result.identifier.clone()).or_insert(0) += 1;
                                        }
                                    }

                                    if !qualified_paths.is_empty() {
                                        println!("💡 Hint: Found {} struct literal(s) with fully qualified paths:",
                                            qualified_paths.values().sum::<usize>());

                                        let mut paths: Vec<_> = qualified_paths.iter().collect();
                                        paths.sort_by_key(|(path, _)| *path);

                                        for (path, count) in &paths {
                                            println!("   {} ({} instance{})", path, count, if **count == 1 { "" } else { "s" });
                                        }

                                        println!("\nTo find only these:");
                                        for (path, _) in paths.iter().take(3) {
                                            println!("   rs-hack find --name \"{}\" --node-type struct-literal --paths ...", path);
                                        }
                                        if paths.len() > 3 {
                                            println!("   (and {} more...)", paths.len() - 3);
                                        }
                                        println!();
                                    }
                                }
                            }
                        }
                    } else {
                        // Standard non-grouped output
                        for result in &all_results {
                            println!("// {}:{}:{} - {}",
                                result.file_path,
                                result.location.line,
                                result.location.column,
                                result.identifier);
                            // Show preceding comment if present
                            if let Some(ref comment) = result.preceding_comment {
                                println!("{}", comment);
                            }
                            println!("{}\n", result.snippet);
                        }
                    }
                }
                _ => {
                    anyhow::bail!("Unknown format: {}. Use 'json', 'locations', or 'snippets'", format);
                }
            }
        }

        HackCommands::AddDerive { paths, target_type, name, derives, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let derive_vec: Vec<String> = derives
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();

            let op = Operation::AddDerive(AddDeriveOp {
                target_name: name.clone(),
                target_type: target_type.clone(),
                derives: derive_vec,
                where_filter: cli.r#where.clone(),
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::AddImplMethod { paths, target, method, position, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            let op = Operation::AddImplMethod(AddImplMethodOp {
                target: target.clone(),
                method_def: method.clone(),
                position: parse_position(&position)?,
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::AddUse { paths, use_path, position, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            let op = Operation::AddUseStatement(AddUseStatementOp {
                use_path: use_path.clone(),
                position: parse_position(&position)?,
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::Add { paths, name, field, field_name, field_type, field_value, variant, method, derive, r#use, match_arm, body, function, auto_detect, enum_name, doc_comment, kind, node_type, literal_default, literal_only, default_rest, base, call, arg, arg_position, call_type, content_filter, position, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            // Handle --call operations first (add argument to function/method calls)
            if let Some(call_name) = call {
                let arg_expr = arg.ok_or_else(|| anyhow::anyhow!("--arg is required when using --call"))?;
                let position = arg_position.parse::<ArgPosition>()
                    .map_err(|e| anyhow::anyhow!("{}", e))?;

                let op = Operation::AddCallArg(AddCallArgOp {
                    call_name,
                    arg_expr,
                    position,
                    call_type,
                    content_filter,
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                return Ok(());
            }

            // Handle --default-rest or --base first (before other operation counting)
            if default_rest || base.is_some() {
                let target_name = name.as_ref().ok_or_else(|| anyhow::anyhow!("--name is required when using --default-rest or --base"))?;

                let base_expr = if default_rest {
                    "Default::default()".to_string()
                } else {
                    base.clone().unwrap()
                };

                let op = Operation::SetStructLiteralBase(SetStructLiteralBaseOp {
                    struct_name: target_name.clone(),
                    base_expr,
                    struct_path: None,
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                return Ok(());
            }

            // Count how many operation flags are set
            let op_count = [field.is_some(), field_name.is_some(), variant.is_some(), method.is_some(), derive.is_some(), r#use.is_some(), match_arm.is_some() || auto_detect, doc_comment.is_some()].iter().filter(|&&x| x).count();

            if op_count == 0 {
                anyhow::bail!("Must specify one of: --field/--field-name, --variant, --method, --derive, --use, --match-arm, --default-rest, --base, --call, or --doc-comment");
            }

            if op_count > 1 {
                // Special hint for common mistake: using --variant with --field-name
                if variant.is_some() && (field_name.is_some() || field.is_some()) {
                    anyhow::bail!(
                        "Cannot combine --variant with --field-name/--field.\n\n\
                         Hint: To add a field to enum variant struct literals, use:\n  \
                         rs-hack add --name \"{}::{}\" --field-name <FIELD> --field-value <VALUE> --kind struct --paths <PATHS>\n\n\
                         Note: --variant is for adding a NEW variant to an enum, not for adding fields to existing variants.",
                        name.as_deref().unwrap_or("EnumName"),
                        variant.as_deref().unwrap_or("VariantName")
                    );
                }
                anyhow::bail!("Can only specify one operation flag at a time (--field/--field-name, --variant, --method, --derive, --use, --match-arm, --call, or --doc-comment)");
            }

            // Handle --match-arm operations
            if match_arm.is_some() || auto_detect {
                if body.is_none() {
                    anyhow::bail!("--body is required when using --match-arm or --auto-detect");
                }

                if auto_detect {
                    if enum_name.is_none() {
                        anyhow::bail!("--enum-name is required when using --auto-detect");
                    }

                    // Warn if --match-arm was also specified (it will be ignored)
                    if match_arm.is_some() {
                        eprintln!("⚠️  Note: --match-arm is ignored with --auto-detect. Auto-detect adds ALL missing variants.");
                        eprintln!("   To add a specific arm only, remove --auto-detect.");
                    }

                    let op = Operation::AddMatchArm(AddMatchArmOp {
                        pattern: "".to_string(), // Not used in auto-detect mode
                        body: body.clone().unwrap(),
                        function_name: function.clone(),
                        auto_detect: true,
                        enum_name: enum_name.clone(),
                    });
                    execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                } else {
                    let op = Operation::AddMatchArm(AddMatchArmOp {
                        pattern: match_arm.clone().unwrap(),
                        body: body.clone().unwrap(),
                        function_name: function.clone(),
                        auto_detect: false,
                        enum_name: None,
                    });
                    execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                }
                return Ok(());
            }

            // Handle --doc-comment operations
            if let Some(doc_text) = doc_comment {
                if name.is_none() {
                    anyhow::bail!("--name is required when using --doc-comment");
                }

                // Resolve node_type from kind if provided
                let target_type = if let Some(k) = &kind {
                    let expanded = expand_kind_to_node_types(k);
                    if expanded.is_empty() {
                        anyhow::bail!("Unknown kind '{}'. Valid kinds: struct, function, enum, match, identifier, type, macro, const, trait, mod, use", k);
                    }
                    if expanded.len() > 1 {
                        anyhow::bail!("Kind '{}' expands to multiple node types. Use --node-type for doc comments to specify exactly which type.", k);
                    }
                    expanded[0].to_string()
                } else if let Some(nt) = &node_type {
                    nt.clone()
                } else {
                    anyhow::bail!("--node-type or --kind is required when using --doc-comment");
                };

                let op = Operation::AddDocComment(AddDocCommentOp {
                    target_type,
                    name: name.clone().unwrap(),
                    doc_comment: doc_text.clone(),
                    style: DocCommentStyle::Line,
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                return Ok(());
            }

            // Handle --use (doesn't require --name)
            if let Some(use_path) = r#use {
                let op = Operation::AddUseStatement(AddUseStatementOp {
                    use_path: use_path.clone(),
                    position: parse_position(&position)?,
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                return Ok(());
            }

            // All other operations require --name
            let target_name = name.as_ref().ok_or_else(|| anyhow::anyhow!("--name is required for this operation"))?;

            // Auto-detect operation type and execute
            // Handle both old --field API and new --field-name API
            if field.is_some() || field_name.is_some() {
                // Convert new API to internal format
                let (final_field_def, final_literal_default) = if let Some(fname) = field_name {
                    // New unified API: --field-name + --field-type + --field-value
                    match (field_type.as_ref(), field_value.as_ref()) {
                        (Some(ftype), Some(fvalue)) => {
                            // Both type and value: definition + literals
                            (format!("{}: {}", fname, ftype), Some(fvalue.clone()))
                        }
                        (Some(ftype), None) => {
                            // Only type: definition only
                            (format!("{}: {}", fname, ftype), None)
                        }
                        (None, Some(fvalue)) => {
                            // Only value: literals only (no type needed)
                            (fname.clone(), Some(fvalue.clone()))
                        }
                        (None, None) => {
                            anyhow::bail!("--field-name requires either --field-type (for definitions) or --field-value (for literals) or both");
                        }
                    }
                } else {
                    // Old API: --field [--literal-default]
                    (field.clone().unwrap(), literal_default.clone())
                };

                // For literal-only operations (field_value without field_type), skip struct definition check
                // Only check if struct exists when we're modifying the definition
                let is_literal_only = field_type.is_none() && field_value.is_some();

                if !is_literal_only {
                    // Check if target exists using kind expansion if provided
                    let exists = if let Some(k) = &kind {
                        // Use kind expansion to check multiple node types
                        let node_types = expand_kind_to_node_types(k);
                        let mut found = false;
                        for nt in node_types {
                            if target_exists(&files, target_name, Some(nt))? {
                                found = true;
                                break;
                            }
                        }
                        found
                    } else if let Some(nt) = &node_type {
                        // Use specific node type
                        target_exists(&files, target_name, Some(nt))?
                    } else {
                        // Default to struct
                        target_exists(&files, target_name, Some("struct"))?
                    };

                    if !exists {
                        show_target_hints(&files, target_name, "struct", &paths)?;
                        return Ok(());
                    }
                }

                let op = Operation::AddStructField(AddStructFieldOp {
                    struct_name: target_name.clone(),
                    field_def: final_field_def,
                    position: parse_position(&position)?,
                    literal_default: final_literal_default,
                    where_filter: cli.r#where.clone(),
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
            } else if let Some(variant_def) = variant {
                // Adding enum variant
                if !target_exists(&files, target_name, Some("enum"))? {
                    show_target_hints(&files, target_name, "enum", &paths)?;
                    return Ok(());
                }

                let op = Operation::AddEnumVariant(AddEnumVariantOp {
                    enum_name: target_name.clone(),
                    variant_def: variant_def.clone(),
                    position: parse_position(&position)?,
                    where_filter: cli.r#where.clone(),
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
            } else if let Some(method_def) = method {
                // Adding impl method
                // Note: impl methods target the type name, not "impl TypeName"
                if !target_exists(&files, target_name, None)? {
                    show_target_hints(&files, target_name, "impl", &paths)?;
                    return Ok(());
                }

                let op = Operation::AddImplMethod(AddImplMethodOp {
                    target: target_name.clone(),
                    method_def: method_def.clone(),
                    position: parse_position(&position)?,
                });
                execute_operation(&files, &op, apply, None, &cli.format, cli.summary, cli.limit)?;
            } else if let Some(derives) = derive {
                // Adding derive macros
                // Need to detect if target is struct or enum
                let target_type = detect_target_type(&files, target_name)?;

                if target_type.is_none() {
                    show_target_hints(&files, target_name, "struct or enum", &paths)?;
                    return Ok(());
                }

                let derive_vec: Vec<String> = derives
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect();

                let op = Operation::AddDerive(AddDeriveOp {
                    target_name: target_name.clone(),
                    target_type: target_type.unwrap(),
                    derives: derive_vec,
                    where_filter: cli.r#where.clone(),
                });
                execute_operation(&files, &op, apply, None, &cli.format, cli.summary, cli.limit)?;
            }
        }

        HackCommands::Remove { paths, name, field_name, variant, method, derive, match_arm, function, doc_comment, kind, node_type, literal_only, call, arg_index, call_type, content_filter, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            // Handle --call operations first (remove argument from function/method calls)
            if let Some(call_name) = call {
                let idx = arg_index.ok_or_else(|| anyhow::anyhow!("--arg-index is required when using --call"))?;

                let op = Operation::RemoveCallArg(RemoveCallArgOp {
                    call_name,
                    arg_index: idx,
                    call_type,
                    content_filter,
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                return Ok(());
            }

            // Count how many operation flags are set
            let op_count = [field_name.is_some(), variant.is_some(), method.is_some(), derive.is_some(), match_arm.is_some(), doc_comment].iter().filter(|&&x| x).count();

            if op_count == 0 {
                anyhow::bail!("Must specify one of: --field-name, --variant, --method, --derive, --match-arm, --call, or --doc-comment");
            }

            if op_count > 1 {
                anyhow::bail!("Can only specify one operation flag at a time (--field-name, --variant, --method, --derive, --match-arm, --call, or --doc-comment)");
            }

            // Handle --match-arm operations
            if let Some(pattern) = match_arm {
                let op = Operation::RemoveMatchArm(RemoveMatchArmOp {
                    pattern: pattern.clone(),
                    function_name: function.clone(),
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                return Ok(());
            }

            // Handle --doc-comment operations
            if doc_comment {
                if name.is_none() {
                    anyhow::bail!("--name is required when using --doc-comment");
                }

                // Resolve node_type from kind if provided
                let target_type = if let Some(k) = &kind {
                    let expanded = expand_kind_to_node_types(k);
                    if expanded.is_empty() {
                        anyhow::bail!("Unknown kind '{}'. Valid kinds: struct, function, enum, match, identifier, type, macro, const, trait, mod, use", k);
                    }
                    if expanded.len() > 1 {
                        anyhow::bail!("Kind '{}' expands to multiple node types. Use --node-type for doc comments to specify exactly which type.", k);
                    }
                    expanded[0].to_string()
                } else if let Some(nt) = &node_type {
                    nt.clone()
                } else {
                    anyhow::bail!("--node-type or --kind is required when using --doc-comment");
                };

                let op = Operation::RemoveDocComment(RemoveDocCommentOp {
                    target_type,
                    name: name.clone().unwrap(),
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                return Ok(());
            }

            // All other operations require --name
            if name.is_none() {
                anyhow::bail!("--name is required for this operation");
            }
            let target_name = name.as_ref().unwrap();

            // Auto-detect operation type and execute
            if let Some(field) = field_name {
                // Removing struct/enum-variant field
                // Check if target exists using kind expansion if provided (skip for literal-only)
                let exists = if literal_only {
                    // For literal-only operations, skip definition check
                    true
                } else if let Some(k) = &kind {
                    // Use kind expansion to check multiple node types
                    let node_types = expand_kind_to_node_types(k);
                    let mut found = false;
                    for nt in node_types {
                        if target_exists(&files, target_name, Some(nt))? {
                            found = true;
                            break;
                        }
                    }
                    found
                } else if let Some(nt) = &node_type {
                    // Use specific node type
                    target_exists(&files, target_name, Some(nt))?
                } else {
                    // Legacy detection: check for :: for enum variants
                    if target_name.contains("::") {
                        // Parse as enum variant (EnumName::VariantName)
                        let parts: Vec<&str> = target_name.split("::").collect();
                        if parts.len() == 2 {
                            let enum_name = parts[0];
                            target_exists(&files, enum_name, Some("enum"))?
                        } else {
                            anyhow::bail!("Invalid enum variant syntax. Use EnumName::VariantName");
                        }
                    } else {
                        // Default to struct
                        target_exists(&files, target_name, Some("struct"))?
                    }
                };

                if !exists {
                    show_target_hints(&files, target_name, "struct", &paths)?;
                    return Ok(());
                }

                let op = Operation::RemoveStructField(RemoveStructFieldOp {
                    struct_name: target_name.clone(),
                    field_name: field.clone(),
                    literal_only,
                    where_filter: cli.r#where.clone(),
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
            } else if let Some(variant_name) = variant {
                // Removing enum variant
                if !target_exists(&files, target_name, Some("enum"))? {
                    show_target_hints(&files, target_name, "enum", &paths)?;
                    return Ok(());
                }

                let op = Operation::RemoveEnumVariant(RemoveEnumVariantOp {
                    enum_name: target_name.clone(),
                    variant_name: variant_name.clone(),
                    where_filter: cli.r#where.clone(),
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
            } else if let Some(method_name) = method {
                // Removing impl method
                // Note: We don't have RemoveImplMethod operation yet, so bail with helpful message
                anyhow::bail!("Remove impl method is not yet implemented. Use the transform command to comment out methods:\n  rs-hack transform --paths src --node-type impl-method --name {} --action comment --apply", method_name);
            } else if let Some(derive_macro) = derive {
                // Removing derive macro
                // Note: We don't have RemoveDerive operation yet, so bail with helpful message
                anyhow::bail!("Remove derive macro is not yet implemented. This is planned for a future release.\nFor now, you can manually edit the derive attribute or use the transform command.");
            }
        }

        HackCommands::Update { paths, name, field, field_name, field_type, variant, match_arm, body, function, doc_comment, kind, node_type, call, arg_index, arg, call_type, content_filter, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            // Handle --call operations first (update argument in function/method calls)
            if let Some(call_name) = call {
                let idx = arg_index.ok_or_else(|| anyhow::anyhow!("--arg-index is required when using --call"))?;
                let new_expr = arg.ok_or_else(|| anyhow::anyhow!("--arg is required when using --call"))?;

                let op = Operation::UpdateCallArg(UpdateCallArgOp {
                    call_name,
                    arg_index: idx,
                    new_expr,
                    call_type,
                    content_filter,
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                return Ok(());
            }

            // Count how many operation flags are set
            let op_count = [field.is_some(), field_name.is_some(), variant.is_some(), match_arm.is_some(), doc_comment.is_some()].iter().filter(|&&x| x).count();

            if op_count == 0 {
                anyhow::bail!("Must specify one of: --field, --variant, --match-arm, --call, or --doc-comment");
            }

            if op_count > 1 {
                anyhow::bail!("Can only specify one operation flag at a time (--field, --variant, --match-arm, --call, or --doc-comment)");
            }

            // Handle --match-arm operations
            if let Some(pattern) = match_arm {
                if body.is_none() {
                    anyhow::bail!("--body is required when using --match-arm");
                }

                let op = Operation::UpdateMatchArm(UpdateMatchArmOp {
                    pattern: pattern.clone(),
                    new_body: body.clone().unwrap(),
                    function_name: function.clone(),
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                return Ok(());
            }

            // Handle --doc-comment operations
            if let Some(doc_text) = doc_comment {
                if name.is_none() {
                    anyhow::bail!("--name is required when using --doc-comment");
                }

                // Resolve node_type from kind if provided
                let target_type = if let Some(k) = &kind {
                    let expanded = expand_kind_to_node_types(k);
                    if expanded.is_empty() {
                        anyhow::bail!("Unknown kind '{}'. Valid kinds: struct, function, enum, match, identifier, type, macro, const, trait, mod, use", k);
                    }
                    if expanded.len() > 1 {
                        anyhow::bail!("Kind '{}' expands to multiple node types. Use --node-type for doc comments to specify exactly which type.", k);
                    }
                    expanded[0].to_string()
                } else if let Some(nt) = &node_type {
                    nt.clone()
                } else {
                    anyhow::bail!("--node-type or --kind is required when using --doc-comment");
                };

                let op = Operation::UpdateDocComment(UpdateDocCommentOp {
                    target_type,
                    name: name.clone().unwrap(),
                    doc_comment: doc_text.clone(),
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
                return Ok(());
            }

            // All other operations require --name
            if name.is_none() {
                anyhow::bail!("--name is required for this operation");
            }
            let target_name = name.as_ref().unwrap();

            // Auto-detect operation type and execute
            // Handle both old --field API and new --field-name API
            if field.is_some() || field_name.is_some() {
                // Convert new API to internal format
                let final_field_def = if let Some(fname) = field_name {
                    // New unified API: --field-name + --field-type
                    if let Some(ftype) = field_type {
                        format!("{}: {}", fname, ftype)
                    } else {
                        anyhow::bail!("--field-name requires --field-type for UPDATE operation");
                    }
                } else {
                    // Old API: --field
                    field.clone().unwrap()
                };

                // Check if target exists using kind expansion if provided
                let exists = if let Some(k) = &kind {
                    // Use kind expansion to check multiple node types
                    let node_types = expand_kind_to_node_types(k);
                    let mut found = false;
                    for nt in node_types {
                        if target_exists(&files, target_name, Some(nt))? {
                            found = true;
                            break;
                        }
                    }
                    found
                } else if let Some(nt) = &node_type {
                    // Use specific node type
                    target_exists(&files, target_name, Some(nt))?
                } else {
                    // Default to struct
                    target_exists(&files, target_name, Some("struct"))?
                };

                if !exists {
                    show_target_hints(&files, target_name, "struct", &paths)?;
                    return Ok(());
                }

                let op = Operation::UpdateStructField(UpdateStructFieldOp {
                    struct_name: target_name.clone(),
                    field_def: final_field_def,
                    where_filter: cli.r#where.clone(),
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
            } else if let Some(variant_def) = variant {
                // Updating enum variant
                if !target_exists(&files, target_name, Some("enum"))? {
                    show_target_hints(&files, target_name, "enum", &paths)?;
                    return Ok(());
                }

                let op = Operation::UpdateEnumVariant(UpdateEnumVariantOp {
                    enum_name: target_name.clone(),
                    variant_def: variant_def.clone(),
                    where_filter: cli.r#where.clone(),
                });
                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
            }
        }

        HackCommands::History { limit } => {
            let state_dir = get_state_dir(cli.local_state)?;
            show_history(limit, &state_dir)?;
        }

        HackCommands::Revert { run_id, force } => {
            let state_dir = get_state_dir(cli.local_state)?;
            revert_run(&run_id, force, &state_dir)?;
        }

        HackCommands::Clean { keep_days } => {
            let state_dir = get_state_dir(cli.local_state)?;
            clean_old_state(keep_days, &state_dir)?;
        }

        HackCommands::Transform { paths, node_type, name, content_filter, action, with, apply } => {
            use operations::{TransformOp, TransformAction};

            // Parse the action
            let transform_action = match action.as_str() {
                "comment" => TransformAction::Comment,
                "remove" => TransformAction::Remove,
                "replace" => {
                    let replacement = with.ok_or_else(|| anyhow::anyhow!("--with is required when action is 'replace'"))?;
                    TransformAction::Replace { with: replacement }
                }
                _ => anyhow::bail!("Invalid action: {}. Use 'comment', 'remove', or 'replace'", action),
            };

            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::Transform(TransformOp {
                node_type: node_type.clone(),
                name_filter: name,
                content_filter,
                action: transform_action,
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::AddDocComment { paths, target_type, name, doc_comment, style, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            // Parse style
            let doc_style = style.parse::<DocCommentStyle>()
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            let op = Operation::AddDocComment(AddDocCommentOp {
                target_type: target_type.clone(),
                name: name.clone(),
                doc_comment: doc_comment.clone(),
                style: doc_style,
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::UpdateDocComment { paths, target_type, name, doc_comment, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            let op = Operation::UpdateDocComment(UpdateDocCommentOp {
                target_type: target_type.clone(),
                name: name.clone(),
                doc_comment: doc_comment.clone(),
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::RemoveDocComment { paths, target_type, name, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            let op = Operation::RemoveDocComment(RemoveDocCommentOp {
                target_type: target_type.clone(),
                name: name.clone(),
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary, cli.limit)?;
        }

        HackCommands::FindField { paths, field_name, summary } => {
            use operations::{FieldLocation, FieldContext};

            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            let mut all_struct_defs = Vec::new();
            let mut all_enum_variants = Vec::new();
            let mut all_literals = Vec::new();

            for file in files {
                let content = std::fs::read_to_string(&file)?;
                let editor = match RustEditor::new(&content) {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("⚠️  Skipping {}: {}", file.display(), e);
                        continue;
                    }
                };
                let locations = editor.find_field_locations(&field_name)?;

                for mut loc in locations {
                    loc.file_path = file.to_string_lossy().to_string();
                    match loc.context {
                        FieldContext::StructDefinition { .. } => all_struct_defs.push(loc),
                        FieldContext::EnumVariantDefinition { .. } => all_enum_variants.push(loc),
                        FieldContext::StructLiteral { .. } => all_literals.push(loc),
                    }
                }
            }

            // Print results
            if all_struct_defs.is_empty() && all_enum_variants.is_empty() && all_literals.is_empty() {
                println!("No occurrences of field '{}' found.", field_name);
                return Ok(());
            }

            println!("Found field \"{}\" in:\n", field_name);

            if !all_struct_defs.is_empty() {
                println!("Struct definitions:");
                for loc in &all_struct_defs {
                    if let FieldContext::StructDefinition { struct_name, field_type } = &loc.context {
                        println!("  - {}.{}: {} ({}:{})", struct_name, field_name, field_type, loc.file_path, loc.line);
                    }
                }
                println!();
            }

            if !all_enum_variants.is_empty() {
                println!("Enum variant definitions:");
                for loc in &all_enum_variants {
                    if let FieldContext::EnumVariantDefinition { enum_name, variant_name, field_type } = &loc.context {
                        println!("  - {}::{}.{}: {} ({}:{})", enum_name, variant_name, field_name, field_type, loc.file_path, loc.line);
                    }
                }
                println!();
            }

            if !all_literals.is_empty() {
                println!("Struct literal expressions: ({} occurrences)", all_literals.len());
                let to_show = if summary { 5 } else { all_literals.len() };
                for loc in all_literals.iter().take(to_show) {
                    if let FieldContext::StructLiteral { struct_name } = &loc.context {
                        println!("  - {} ({}:{})", struct_name, loc.file_path, loc.line);
                    }
                }
                if all_literals.len() > to_show {
                    println!("  ... ({} more)", all_literals.len() - to_show);
                }
                println!();
            }

            // Print suggested commands
            if !all_struct_defs.is_empty() || !all_enum_variants.is_empty() {
                println!("Suggested commands:");
                for loc in &all_struct_defs {
                    if let FieldContext::StructDefinition { struct_name, .. } = &loc.context {
                        println!("  # Remove from struct definition AND all literals");
                        println!("  rs-hack remove-struct-field --struct-name \"{}\" --field-name \"{}\" --paths src --apply", struct_name, field_name);
                        println!();
                    }
                }
                for loc in &all_enum_variants {
                    if let FieldContext::EnumVariantDefinition { enum_name, variant_name, .. } = &loc.context {
                        println!("  # Remove from enum variant definition AND all literals");
                        println!("  rs-hack remove-struct-field --struct-name \"{}::{}\" --field-name \"{}\" --paths src --apply", enum_name, variant_name, field_name);
                        println!();
                    }
                }
            }
        }

        }

        Commands::Arch(arch_cmd) => {
            handle_arch_command(arch_cmd)?;
        }

        Commands::Board(board_cmd) => {
            handle_board_command(board_cmd)?;
        }

        Commands::Mcp { transport } => {
            if transport != "stdio" {
                bail!("only stdio transport is supported (got: {})", transport);
            }
            handle_mcp_command()?;
        }

        Commands::Migrate { path, apply } => {
            handle_migrate_command(&path, apply)?;
        }
    }

    Ok(())
}

fn handle_migrate_command(workspace: &Path, apply: bool) -> Result<()> {
    use std::collections::BTreeMap;

    let workspace = std::fs::canonicalize(workspace)
        .with_context(|| format!("workspace not found: {}", workspace.display()))?;

    let scan_exts = [
        "rs", "ts", "tsx", "js", "jsx", "md", "toml", "yaml", "yml",
    ];
    let scan_named = ["CLAUDE.md", "README.md", "AGENTS.md"];

    let mut changes: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for entry in WalkDir::new(&workspace)
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !(n == "target"
                || n == "node_modules"
                || n == ".git"
                || n == ".yah"
                || n == ".hack")
        })
        .filter_map(|r| r.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        let ext_ok = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| scan_exts.contains(&e))
            .unwrap_or(false);
        let name_ok = p
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| scan_named.contains(&n))
            .unwrap_or(false);
        if !ext_ok && !name_ok {
            continue;
        }

        // Constructed from parts so this tool stays migratable from a future
        // legacy prefix without self-rewriting. Don't inline the literal.
        let from = format!("@{}:", "hack");
        let to = format!("@{}:", "yah");
        let Ok(content) = std::fs::read_to_string(p) else {
            continue;
        };
        let count = content.matches(from.as_str()).count();
        if count == 0 {
            continue;
        }
        changes.insert(p.to_path_buf(), count);
        if apply {
            let new = content.replace(from.as_str(), to.as_str());
            std::fs::write(p, new)
                .with_context(|| format!("failed to write {}", p.display()))?;
        }
    }

    let total: usize = changes.values().sum();
    let action = if apply { "rewrote" } else { "would rewrite" };
    println!(
        "{} `@{}:` → `@{}:` ({} occurrences across {} files)",
        action,
        "hack",
        "yah",
        total,
        changes.len()
    );
    for (path, n) in &changes {
        let rel = path.strip_prefix(&workspace).unwrap_or(path);
        println!("  {:>4}  {}", n, rel.display());
    }

    let hack_dir = workspace.join(".hack");
    let yah_dir = workspace.join(".yah");
    if hack_dir.exists() {
        if yah_dir.exists() {
            eprintln!(
                "\n⚠ both .hack/ and .yah/ exist at {} — refusing to merge automatically.\n  \
                 Inspect both, move what you need into .yah/, then `rm -rf .hack/`.",
                workspace.display()
            );
        } else if apply {
            std::fs::rename(&hack_dir, &yah_dir).with_context(|| {
                format!(
                    "failed to rename {} → {}",
                    hack_dir.display(),
                    yah_dir.display()
                )
            })?;
            println!("\nrenamed .hack/ → .yah/");
        } else {
            println!("\nwould rename .hack/ → .yah/");
        }
    }

    if !apply {
        println!("\n(dry-run) re-run with --apply to write changes");
    }

    Ok(())
}

fn handle_mcp_command() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()
        .context("failed to start tokio runtime for MCP server")?;
    rt.block_on(async {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_target(false)
            .try_init()
            .ok();
        let server = mcp::Server::new();
        server.run().await
    })
}

fn handle_board_command(cmd: BoardCommands) -> Result<()> {
    match cmd {
        BoardCommands::Serve { path, port, udp_port, open } => {
            let workspace = std::fs::canonicalize(&path)
                .unwrap_or_else(|_| path.clone());

            // Auto-pick port pair from workspace hash if not explicitly set.
            // Mirrors the logic in hack-board/src/server.ts so `rs-hack board serve`
            // and `bun run src/server.ts` pick the same port for the same workspace.
            let auto_port = {
                let s = workspace.to_string_lossy();
                let mut h: u32 = 5381;
                for b in s.bytes() {
                    h = h.wrapping_mul(33).wrapping_add(b as u32);
                }
                3333 + ((h % 333) as u16) * 2
            };
            let http_port = port.unwrap_or(auto_port);
            let udp = udp_port.unwrap_or(http_port + 1);

            // Find hack-board directory (shipped alongside rs-hack, or in the rs-hack source)
            let hack_board_dir = find_hack_board_dir();

            match hack_board_dir {
                Some(dir) => {
                    eprintln!("hack-board: http://localhost:{}", http_port);
                    eprintln!("workspace:  {}", workspace.display());
                    eprintln!("server:     {}", dir.display());

                    if open {
                        let _ = std::process::Command::new("open")
                            .arg(format!("http://localhost:{}", http_port))
                            .spawn();
                    }

                    let status = std::process::Command::new("bun")
                        .arg("run")
                        .arg("src/server.ts")
                        .current_dir(&dir)
                        .env("HACK_WORKSPACE", workspace.to_string_lossy().as_ref())
                        .env("HACK_PORT", http_port.to_string())
                        .env("HACK_UDP_PORT", udp.to_string())
                        .env("YAH_BIN", std::env::current_exe().unwrap_or_else(|_| "yah".into()))
                        .env("RS_HACK_BIN", std::env::current_exe().unwrap_or_else(|_| "yah".into()))
                        .status();

                    match status {
                        Ok(s) if !s.success() => {
                            eprintln!("hack-board exited with: {}", s);
                        }
                        Err(e) => {
                            eprintln!("Failed to start bun: {}", e);
                            eprintln!("Install bun: https://bun.sh");
                            std::process::exit(1);
                        }
                        _ => {}
                    }
                }
                None => {
                    eprintln!("Could not find hack-board directory.");
                    eprintln!("Expected at: <rs-hack-source>/hack-board/");
                    eprintln!("Make sure rs-hack was installed from source with the hack-board directory present.");
                    std::process::exit(1);
                }
            }
        }

        BoardCommands::Init { path, force } => {
            handle_init(&path, force)?;
        }

        BoardCommands::Prompt { name } => {
            handle_prompt(name.as_deref())?;
        }

        BoardCommands::Show { id, path, prompt, format } => {
            use arch::extract::extract_from_workspace_verbose;
            use arch::ticket::TicketBoard;

            let annotations = extract_from_workspace_verbose(&path, false)?;
            let board = TicketBoard::from_annotations(&annotations);

            if let Some(ticket) = board.get(&id) {
                if prompt {
                    let ctx = board.build_prompt_context(&ticket.id);
                    println!("{}", ticket.to_prompt_with_ctx(&ctx));
                    return Ok(());
                }
                match format.as_str() {
                    "json" => println!("{}", serde_json::to_string_pretty(ticket)?),
                    _ => print!("{}", render_show_markdown(ticket, &board)),
                }
                return Ok(());
            }

            // Not on the live board — try the archive shards before giving up.
            if let Some(archived) = arch::archive::lookup(&path, &id) {
                if prompt {
                    eprintln!("Ticket '{}' is archived; --prompt is only meaningful for live tickets.", id);
                    eprintln!("Showing the archived snapshot instead:\n");
                }
                match format.as_str() {
                    "json" => println!("{}", serde_json::to_string_pretty(&archived.ticket)?),
                    _ => print!("{}", render_archived_markdown(&archived)),
                }
                return Ok(());
            }

            eprintln!("Ticket '{}' not found on live board or in archive shards.", id);
            let suggestions = fuzzy_suggest(&id, &board);
            if !suggestions.is_empty() {
                eprintln!("Did you mean: {}", suggestions.join(", "));
            }
            eprintln!("List all live tickets: `rs-hack board tickets`");
            std::process::exit(1);
        }

        BoardCommands::Tickets { path, format, status, assignee, epics, prompt, relay_doc } => {
            use arch::extract::extract_from_workspace_verbose;
            use arch::ticket::{TicketBoard, TicketStatus};

            let annotations = extract_from_workspace_verbose(&path, false)?;
            let board = TicketBoard::from_annotations(&annotations);

            // --prompt R001: generate continuation prompt for a ticket.
            // Build full hierarchy context (live children + grandchild counts
            // for epics + parent for sub-tickets) so the prompt adapts its
            // shape — relay-with-subtickets hoists the Rule08 cycle, epics
            // walk one level deeper, sub-tickets inherit parent gotchas.
            if let Some(ref ticket_id) = prompt {
                match board.get(ticket_id) {
                    Some(ticket) => {
                        let ctx = board.build_prompt_context(&ticket.id);
                        println!("{}", ticket.to_prompt_with_ctx(&ctx));
                        return Ok(());
                    }
                    None => {
                        if let Some(archived) = arch::archive::lookup(&path, ticket_id) {
                            eprintln!("Ticket '{}' is archived (last status: {}). Pickup prompt is only meaningful for live tickets.", ticket_id, archived.ticket.status);
                            std::process::exit(1);
                        }
                        eprintln!("Ticket '{}' not found.", ticket_id);
                        let hits = fuzzy_suggest(ticket_id, &board);
                        if !hits.is_empty() {
                            eprintln!("Did you mean: {}", hits.join(", "));
                        }
                        std::process::exit(1);
                    }
                }
            }

            // --relay-doc R001: generate relay markdown doc
            if let Some(ref ticket_id) = relay_doc {
                match board.get(ticket_id) {
                    Some(ticket) => {
                        println!("{}", ticket.to_relay_doc());
                        return Ok(());
                    }
                    None => {
                        eprintln!("Ticket '{}' not found", ticket_id);
                        std::process::exit(1);
                    }
                }
            }

            // Apply filters
            let tickets: Vec<_> = board.tickets.iter().filter(|t| {
                if epics && !t.is_epic {
                    return false;
                }
                if let Some(ref sf) = status {
                    let expected = TicketStatus::parse(sf);
                    if t.status != expected {
                        return false;
                    }
                }
                if let Some(ref af) = assignee {
                    if t.assignee.as_deref() != Some(af.as_str()) {
                        return false;
                    }
                }
                true
            }).collect();

            if tickets.is_empty() {
                match format.as_str() {
                    "json" => println!("[]"),
                    _ => println!("No tickets found"),
                }
                return Ok(());
            }

            match format.as_str() {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&tickets)?);
                }
                _ => {
                    let filtered = TicketBoard { tickets: tickets.into_iter().cloned().collect() };
                    println!("{}", filtered.to_markdown());
                }
            }
        }

        BoardCommands::Claim {
            id,
            path,
            kind,
            file,
            title,
            assignee,
            status,
            phase,
            parent,
            severity,
            handoff,
            next,
            verify,
            cleanup,
            gotcha,
            assumes,
            see,
            json,
        } => {
            if let Some(existing_id) = id {
                // Flip-mode: claim an existing Open ticket.
                if status.is_some() {
                    anyhow::bail!(
                        "`--status` is not valid when claiming an existing ticket; \
                         use `rs-hack board move {} <column>` instead.",
                        existing_id
                    );
                }
                if !handoff.is_empty()
                    || !next.is_empty()
                    || !verify.is_empty()
                    || !cleanup.is_empty()
                    || !gotcha.is_empty()
                    || !assumes.is_empty()
                    || !see.is_empty()
                    || phase.is_some()
                    || parent.is_some()
                    || severity.is_some()
                {
                    anyhow::bail!(
                        "`claim <ID>` only flips status and sets assignee. \
                         Use `rs-hack board move {} active --handoff '...' --next '...'` \
                         to attach payload, or edit the annotation directly.",
                        existing_id
                    );
                }
                let resolved_assignee = assignee.unwrap_or_else(current_agent_id);
                handle_claim_existing(&existing_id, &path, &resolved_assignee)?;
            } else {
                // Create-mode: unwrap required-when-creating flags (clap enforced).
                let kind = kind.expect("clap required_unless_present");
                let file = file.expect("clap required_unless_present");
                let title = title.expect("clap required_unless_present");

                // Default-assignee-to-current-agent only when no --status was
                // passed (back-compat: legacy slash commands call
                // `claim --status handoff` and must not get a self-assign).
                let resolved_assignee = if assignee.is_some() {
                    assignee
                } else if status.is_none() {
                    Some(current_agent_id())
                } else {
                    None
                };
                if status.is_some() {
                    eprintln!(
                        "warning: `rs-hack board claim --status` is deprecated. \
                         Use `board open` (open column), bare `board claim` (active), \
                         or `board move <ID> <column>` (transition)."
                    );
                }
                handle_claim(ClaimArgs {
                    path,
                    kind,
                    file,
                    title,
                    assignee: resolved_assignee,
                    status,
                    phase,
                    parent,
                    severity,
                    handoff,
                    next,
                    verify,
                    cleanup,
                    gotcha,
                    assumes,
                    see,
                    json,
                })?;
            }
        }

        BoardCommands::Open {
            path,
            kind,
            file,
            title,
            phase,
            parent,
            severity,
            next,
            verify,
            cleanup,
            see,
            json,
        } => {
            handle_claim(ClaimArgs {
                path,
                kind,
                file,
                title,
                assignee: None,
                status: Some("open".to_string()),
                phase,
                parent,
                severity,
                handoff: Vec::new(),
                next,
                verify,
                cleanup,
                gotcha: Vec::new(),
                assumes: Vec::new(),
                see,
                json,
            })?;
        }

        BoardCommands::Move {
            id,
            column,
            path,
            assignee,
            handoff,
            next,
            verify,
            cleanup,
            gotcha,
            assumes,
        } => {
            handle_move(MoveArgs {
                id,
                column,
                path,
                assignee,
                handoff,
                next,
                verify,
                cleanup,
                gotcha,
                assumes,
            })?;
        }

        BoardCommands::Archive { id, path } => {
            handle_archive(&id, &path)?;
        }

        BoardCommands::Summary { text, ticket, author, path } => {
            let text = if text == "-" {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            } else {
                text
            };

            let summary = arch::summary::write_summary(
                &path,
                &text,
                ticket.as_deref(),
                author.as_deref(),
            )?;

            eprintln!("Summary written: {}", summary.file.display());
            if let Some(ref tid) = summary.ticket {
                eprintln!("Linked to: {}", tid);
            } else {
                eprintln!("No ticket linked (board inbox)");
            }
            println!("{}", summary.id);
        }

        BoardCommands::Comment { text, relay, author, path } => {
            let text = if text == "-" {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            } else {
                text
            };

            let comment = arch::comment::write_comment(
                &path,
                &text,
                relay.as_deref(),
                author.as_deref(),
            )?;

            eprintln!("Comment written: {}", comment.file.display());
            if let Some(ref rid) = comment.relay {
                eprintln!("Attached to relay: {}", rid);
            } else {
                eprintln!("No relay attached (.yah/comments/_inbox)");
            }
            println!("{}", comment.id);
        }

        BoardCommands::Promote { summary_id, file, title, assignee, path, json } => {
            let res = arch::promote::promote_summary(
                &path,
                &summary_id,
                &file,
                title.as_deref(),
                assignee.as_deref(),
            )?;
            if json {
                let out = serde_json::json!({
                    "relay_id": res.relay_id,
                    "relay_title": res.relay_title,
                    "file": res.file.display().to_string(),
                    "line": res.line,
                    "summary_file": res.summary_file.display().to_string(),
                });
                println!("{}", out);
            } else {
                println!("{}", res.relay_id);
                eprintln!(
                    "{}:{}  ← @yah:relay({}, {:?})",
                    res.file.display(), res.line, res.relay_id, res.relay_title
                );
                eprintln!("summary frontmatter updated: {}", res.summary_file.display());
            }
        }

        BoardCommands::Status { id, path, format } => {
            use arch::extract::extract_from_workspace_verbose;
            use arch::status::BoardStatus;
            use arch::ticket::TicketBoard;

            let annotations = extract_from_workspace_verbose(&path, false)?;
            let board = TicketBoard::from_annotations(&annotations);

            // `board status <ID>` is a friendly alias for `board show <ID>` —
            // matches the natural "status of X" mental model without forcing
            // agents to discover a separate verb.
            if let Some(ticket_id) = id {
                if let Some(ticket) = board.get(&ticket_id) {
                    match format.as_str() {
                        "json" => println!("{}", serde_json::to_string_pretty(ticket)?),
                        _ => print!("{}", render_show_markdown(ticket, &board)),
                    }
                    return Ok(());
                }
                if let Some(archived) = arch::archive::lookup(&path, &ticket_id) {
                    match format.as_str() {
                        "json" => println!("{}", serde_json::to_string_pretty(&archived.ticket)?),
                        _ => print!("{}", render_archived_markdown(&archived)),
                    }
                    return Ok(());
                }
                eprintln!("Ticket '{}' not found on live board or in archive shards.", ticket_id);
                let hits = fuzzy_suggest(&ticket_id, &board);
                if !hits.is_empty() {
                    eprintln!("Did you mean: {}", hits.join(", "));
                }
                std::process::exit(1);
            }

            let status = BoardStatus::compute(&board, &path);
            match format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&status)?),
                _ => print!("{}", status.to_markdown()),
            }
        }

        BoardCommands::Inflight {
            path,
            format,
            include_epics,
        } => {
            handle_inflight(&path, &format, include_epics)?;
        }

        BoardCommands::Rules { context, format } => {
            use arch::sdlc;

            let selected: Vec<&sdlc::SdlcRule> = match context.as_deref() {
                Some(label) => match sdlc::Context::parse(label) {
                    Some(ctx) => sdlc::rules_for(ctx),
                    None => {
                        eprintln!(
                            "Unknown context '{}'. Valid: pickup, finishing, new-work, archive, refactor",
                            label
                        );
                        std::process::exit(1);
                    }
                },
                None => sdlc::RULES.iter().collect(),
            };

            match format.as_str() {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&selected)?);
                }
                "terse" => {
                    println!("{}", sdlc::format_markdown(&selected, true));
                }
                _ => {
                    let header = match context.as_deref() {
                        Some(label) => format!("# hack-board rules — {}\n\n", label),
                        None => String::from("# hack-board rules\n\n"),
                    };
                    print!("{}", header);
                    print!("{}", sdlc::format_markdown(&selected, false));
                    println!(
                        "---\n\nFilter by situation: `rs-hack board rules --context pickup|finishing|new-work|archive|refactor`"
                    );
                }
            }
        }

        BoardCommands::Worktrees(cmd) => handle_worktrees(cmd)?,
    }

    Ok(())
}

fn handle_worktrees(cmd: WorktreesCommands) -> Result<()> {
    use worktrees::{Registry, Sibling};

    match cmd {
        WorktreesCommands::Add { target, path } => {
            let workspace = std::fs::canonicalize(&path).unwrap_or(path);
            let mut reg = Registry::load(&workspace)?;
            let sib = parse_sibling_target(&target)?;
            let added = reg.add(sib.clone());
            if added {
                reg.save(&workspace)?;
                println!("added {}", sib.label());
            } else {
                eprintln!("already present: {}", sib.label());
            }
        }
        WorktreesCommands::List { path, format } => {
            let workspace = std::fs::canonicalize(&path).unwrap_or(path);
            let reg = Registry::load(&workspace)?;
            let auto = worktrees::discover_git_worktrees(&workspace);
            match format.as_str() {
                "json" => {
                    let payload = serde_json::json!({
                        "registered": reg.siblings,
                        "auto_discovered": auto,
                    });
                    println!("{}", serde_json::to_string_pretty(&payload)?);
                }
                _ => {
                    if reg.siblings.is_empty() && auto.is_empty() {
                        println!("# No siblings registered.");
                        println!();
                        println!(
                            "Add one with: rs-hack board worktrees add <local-path | user@host:path>"
                        );
                    } else {
                        if !reg.siblings.is_empty() {
                            println!("# Registered ({})", reg.siblings.len());
                            for sib in &reg.siblings {
                                println!("  {}", sib.label());
                            }
                        }
                        if !auto.is_empty() {
                            if !reg.siblings.is_empty() {
                                println!();
                            }
                            println!("# Auto-discovered git worktrees ({})", auto.len());
                            for sib in &auto {
                                println!("  {}", sib.label());
                            }
                        }
                    }
                }
            }
        }
        WorktreesCommands::Remove { target, path } => {
            let workspace = std::fs::canonicalize(&path).unwrap_or(path);
            let mut reg = Registry::load(&workspace)?;
            if reg.remove(&target) {
                reg.save(&workspace)?;
                println!("removed {}", target);
            } else {
                anyhow::bail!(
                    "no registered sibling matched '{}' (auto-discovered git worktrees are not removable here)",
                    target
                );
            }
        }
    }
    Ok(())
}

/// Parse a sibling target string. `user@host:path` (or `host:path`) is
/// remote; anything else is treated as a local path.
///
/// Heuristic for remote: contains `:` and the part before `:` is not a
/// drive letter or local-path-like (no `/`, no `.`).
fn parse_sibling_target(s: &str) -> Result<worktrees::Sibling> {
    if let Some((before, after)) = s.split_once(':') {
        let looks_remote = !before.is_empty()
            && !before.contains('/')
            && !before.starts_with('.')
            && !std::path::Path::new(before).is_absolute();
        if looks_remote {
            return Ok(worktrees::Sibling::Remote {
                host: before.to_string(),
                path: after.to_string(),
            });
        }
    }
    let p = std::path::PathBuf::from(s);
    let canon = std::fs::canonicalize(&p).unwrap_or(p);
    Ok(worktrees::Sibling::Local { path: canon })
}

// ── Onboarding templates (embedded at compile time) ─────────────────────

const TPL_COMMENT: &str = include_str!("../../templates/commands/comment.md");
const TPL_HANDOFF: &str = include_str!("../../templates/commands/handoff.md");
const TPL_REFINE: &str = include_str!("../../templates/commands/refine.md");
const TPL_CLAUDE_MD_SNIPPET: &str =
    include_str!("../../templates/claude-md-hackboard.md");

const CLAUDE_MD_MARKER_START: &str = "<!-- rs-hack:hack-board:start -->";
const CLAUDE_MD_MARKER_END: &str = "<!-- rs-hack:hack-board:end -->";

fn handle_init(path: &Path, force: bool) -> Result<()> {
    let root = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let commands_dir = root.join(".claude").join("commands");
    std::fs::create_dir_all(&commands_dir)?;

    let files: &[(&str, &str)] = &[
        ("comment.md", TPL_COMMENT),
        ("handoff.md", TPL_HANDOFF),
        ("refine.md", TPL_REFINE),
    ];

    let mut wrote = 0usize;
    let mut skipped = 0usize;
    for (name, body) in files {
        let dest = commands_dir.join(name);
        if dest.exists() && !force {
            eprintln!("  skip  .claude/commands/{name} (exists, use --force to overwrite)");
            skipped += 1;
            continue;
        }
        std::fs::write(&dest, body)?;
        eprintln!("  write .claude/commands/{name}");
        wrote += 1;
    }

    // CLAUDE.md — append snippet if not already present; replace between
    // rs-hack markers if --force is set.
    let claude_md = root.join("CLAUDE.md");
    let existing = std::fs::read_to_string(&claude_md).unwrap_or_default();
    let has_markers =
        existing.contains(CLAUDE_MD_MARKER_START) && existing.contains(CLAUDE_MD_MARKER_END);

    if !has_markers {
        let separator = if existing.is_empty() || existing.ends_with("\n\n") {
            ""
        } else if existing.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        let new_content = format!("{existing}{separator}{TPL_CLAUDE_MD_SNIPPET}");
        std::fs::write(&claude_md, new_content)?;
        eprintln!(
            "  {} CLAUDE.md (hack-board section)",
            if existing.is_empty() { "write" } else { "append" }
        );
        wrote += 1;
    } else if force {
        // Replace between markers.
        let start = existing.find(CLAUDE_MD_MARKER_START).unwrap();
        let end = existing.find(CLAUDE_MD_MARKER_END).unwrap()
            + CLAUDE_MD_MARKER_END.len();
        let mut new_content = String::new();
        new_content.push_str(&existing[..start]);
        new_content.push_str(TPL_CLAUDE_MD_SNIPPET.trim_end());
        new_content.push_str(&existing[end..]);
        std::fs::write(&claude_md, new_content)?;
        eprintln!("  rewrite CLAUDE.md (hack-board section, --force)");
        wrote += 1;
    } else {
        eprintln!("  skip  CLAUDE.md (hack-board section already present, use --force to refresh)");
        skipped += 1;
    }

    eprintln!();
    eprintln!("Done. {wrote} written, {skipped} skipped.");
    eprintln!();
    eprintln!("Next: `rs-hack board serve` (auto-picks a port from the workspace path).");
    Ok(())
}

fn render_show_markdown(
    t: &arch::ticket::Ticket,
    board: &arch::ticket::TicketBoard,
) -> String {
    use std::fmt::Write;
    let mut o = String::new();

    let kind_label = if t.is_epic {
        "epic".to_string()
    } else {
        match t.item_type {
            arch::ticket::ItemType::Relay => "relay".to_string(),
            arch::ticket::ItemType::Ticket => {
                t.kind.clone().unwrap_or_else(|| "ticket".to_string())
            }
        }
    };

    let _ = writeln!(o, "# {} — {}", t.id, t.title);
    let _ = writeln!(o);

    let status_str = if t.is_epic {
        format!("epic:{}", t.epic_status.as_deref().unwrap_or("active"))
    } else {
        t.status.to_string()
    };
    let mut status_bits = vec![
        format!("**Status:** {}", status_str),
        format!("**Kind:** {}", kind_label),
    ];
    if let Some(a) = &t.assignee {
        status_bits.push(format!("**Assignee:** {}", a));
    }
    if let Some(p) = &t.phase {
        status_bits.push(format!("**Phase:** {}", p));
    }
    if let Some(p) = &t.parent {
        status_bits.push(format!("**Parent:** {}", p));
    }
    if let Some(s) = &t.severity {
        status_bits.push(format!("**Severity:** {}", s));
    }
    let _ = writeln!(o, "{}", status_bits.join(" · "));
    let _ = writeln!(o);

    let _ = writeln!(o, "**Source:** {}:{}", t.file.display(), t.line);
    if t.files.len() > 1 {
        let _ = writeln!(
            o,
            "  ⚠ declared in {} files (Rule11 — resolve before working):",
            t.files.len()
        );
        for loc in &t.files {
            let _ = writeln!(o, "  - {}:{}", loc.path.display(), loc.line);
        }
    }
    let _ = writeln!(o);

    let smells = arch::smell::ticket_smells(t, board);
    if !smells.is_empty() {
        let _ = writeln!(o, "## ⚠ Smells");
        for s in &smells {
            let _ = writeln!(o, "- {}", s);
        }
        let _ = writeln!(o);
    }

    let section = |o: &mut String, title: &str, items: &[String]| {
        if !items.is_empty() {
            let _ = writeln!(o, "## {}", title);
            for it in items {
                let _ = writeln!(o, "- {}", it);
            }
            let _ = writeln!(o);
        }
    };

    section(&mut o, "Gotchas", &t.gotchas);
    section(&mut o, "Handoff (completed)", &t.handoff);
    section(&mut o, "Next steps", &t.next_steps);
    section(&mut o, "Verify", &t.verify);
    section(&mut o, "Assumes (unverified)", &t.assumes);
    section(&mut o, "Cleanup (deferred)", &t.cleanup);
    section(&mut o, "See also", &t.see_also);

    let children: Vec<&arch::ticket::Ticket> = board
        .tickets
        .iter()
        .filter(|c| c.parent.as_deref() == Some(t.id.as_str()) && c.id != t.id)
        .collect();
    if !children.is_empty() {
        let _ = writeln!(o, "## Children ({})", children.len());
        for c in &children {
            let cstatus = if c.is_epic {
                format!("epic:{}", c.epic_status.as_deref().unwrap_or("active"))
            } else {
                c.status.to_string()
            };
            let assignee = c.assignee.as_deref().unwrap_or("—");
            let _ = writeln!(o, "- **{}** `{}` · {} · {}", c.id, cstatus, assignee, c.title);
        }
        let _ = writeln!(o);
    }

    if !t.conflicts.is_empty() {
        let _ = writeln!(o, "## ⚠ Field conflicts (Rule11)");
        for (field, vals) in &t.conflicts {
            let _ = writeln!(o, "- **{}**:", field);
            for v in vals {
                let _ = writeln!(o, "  - `{}` at {}:{}", v.value, v.path.display(), v.line);
            }
        }
        let _ = writeln!(o);
    }

    let _ = writeln!(
        o,
        "---\n_For the full agent-pickup prompt: `rs-hack board show {} --prompt`_",
        t.id
    );

    o
}

fn render_archived_markdown(a: &arch::archive::ArchivedTicket) -> String {
    use arch::archive::Disposition;
    use std::fmt::Write;
    let mut o = String::new();
    let t = &a.ticket;

    let banner = match a.disposition {
        Disposition::Archived => "🗄️ **ARCHIVED** — terminal state, lifted from source",
        Disposition::Disappeared => "👻 **DISAPPEARED** — annotation vanished without an archive event",
        Disposition::Stale => "⚠ **STALE** — shard exists but no terminal event recorded",
    };
    let _ = writeln!(o, "# {} — {}", t.id, t.title);
    let _ = writeln!(o);
    let _ = writeln!(o, "{}  ·  last seen: t={}", banner, a.last_seen);
    let _ = writeln!(o);
    let _ = writeln!(o, "**Last status:** {}", t.status);
    if let Some(p) = &t.parent {
        let _ = writeln!(o, "**Parent:** {}", p);
    }
    if let Some(asg) = &t.assignee {
        let _ = writeln!(o, "**Assignee:** {}", asg);
    }
    let _ = writeln!(o);
    let _ = writeln!(o, "**Last source:** {}:{}", t.file.display(), t.line);
    let _ = writeln!(o);

    let section = |o: &mut String, title: &str, items: &[String]| {
        if !items.is_empty() {
            let _ = writeln!(o, "## {}", title);
            for it in items {
                let _ = writeln!(o, "- {}", it);
            }
            let _ = writeln!(o);
        }
    };
    section(&mut o, "Handoff (final)", &t.handoff);
    section(&mut o, "Next steps (final)", &t.next_steps);
    section(&mut o, "Verify", &t.verify);
    section(&mut o, "See also", &t.see_also);

    let shard = t.id.split_once('-').map(|(p, _)| p).unwrap_or(t.id.as_str());
    let _ = writeln!(
        o,
        "---\n_Reconstructed from `.yah/events/{}.jsonl`. Live board does not show archived tickets._",
        shard
    );
    o
}

/// Cheap fuzzy did-you-mean: prefix-match, contains, or Levenshtein ≤ 2.
/// Returns up to 5 candidates from the live board + archive shard names,
/// instead of dumping the whole board.
fn fuzzy_suggest(needle: &str, board: &arch::ticket::TicketBoard) -> Vec<String> {
    let needle_up = needle.to_uppercase();
    let mut candidates: Vec<String> =
        board.tickets.iter().map(|t| t.id.clone()).collect();
    // Pull archived relay/ticket IDs from `.yah/events/*.jsonl` so a typo
    // for an archived ID still gets a useful suggestion.
    if let Ok(entries) = std::fs::read_dir(".yah/events") {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    if !stem.starts_with('_') {
                        candidates.push(stem.to_string());
                    }
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();

    let mut scored: Vec<(u32, String)> = Vec::new();
    for id in candidates {
        let id_up = id.to_uppercase();
        let score = if id_up == needle_up {
            0
        } else if id_up.starts_with(&needle_up) {
            1
        } else if id_up.contains(&needle_up) {
            2
        } else if levenshtein(&id_up, &needle_up) <= 2 {
            3
        } else {
            continue;
        };
        scored.push((score, id));
    }
    scored.sort_by_key(|(s, id)| (*s, id.len()));
    scored.into_iter().take(5).map(|(_, id)| id).collect()
}

fn levenshtein(a: &str, b: &str) -> u32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 { return m as u32; }
    if m == 0 { return n as u32; }
    let mut prev: Vec<u32> = (0..=m as u32).collect();
    let mut curr = vec![0u32; m + 1];
    for i in 1..=n {
        curr[0] = i as u32;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (curr[j - 1] + 1)
                .min(prev[j] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

fn handle_prompt(name: Option<&str>) -> Result<()> {
    let prompts: &[(&str, &str, &str)] = &[
        ("refine", TPL_REFINE, "Turn a multi-phase plan into a relay + tickets"),
        ("handoff", TPL_HANDOFF, "Write a structured relay for the next agent"),
        ("comment", TPL_COMMENT, "Log a progress summary to .yah/summaries/"),
    ];

    match name {
        None => {
            println!("Available prompts (run `rs-hack board prompt <name>`):");
            println!();
            for (n, _, desc) in prompts {
                println!("  {:<8} — {}", n, desc);
            }
            println!();
            println!("These are the same templates `board init` writes to .claude/commands/.");
            println!("Use this when the /<name> slash command isn't available in your harness.");
            Ok(())
        }
        Some(n) => {
            for (pn, body, _) in prompts {
                if *pn == n {
                    print!("{body}");
                    return Ok(());
                }
            }
            anyhow::bail!(
                "unknown prompt: {n} — available: {}",
                prompts.iter().map(|(n, _, _)| *n).collect::<Vec<_>>().join(", ")
            );
        }
    }
}

/// Find the hack-board directory relative to the rs-hack binary or source.
fn find_hack_board_dir() -> Option<PathBuf> {
    // Try relative to the binary (installed via cargo install)
    if let Ok(exe) = std::env::current_exe() {
        // Binary is at target/release/rs-hack or ~/.cargo/bin/rs-hack
        // hack-board would be alongside the source
        let candidates = [
            // Running from cargo run (target/debug/ or target/release/)
            exe.parent()?.parent()?.parent()?.join("hack-board"),
            // Installed - check common source locations
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent()?.join("hack-board"),
        ];
        for candidate in &candidates {
            if candidate.join("src/server.ts").exists() {
                return Some(candidate.clone());
            }
        }
    }

    // Try current directory (if running from rs-hack source)
    let cwd = std::env::current_dir().ok()?;
    let cwd_candidate = cwd.join("hack-board");
    if cwd_candidate.join("src/server.ts").exists() {
        return Some(cwd_candidate);
    }

    None
}

fn handle_arch_command(cmd: ArchCommands) -> Result<()> {
    use arch::{
        extract::extract_from_workspace_verbose,
        graph::{ArchGraph, EdgeKind},
        query::{get_file_context, trace_path, Query},
        schema::Schema,
        validate::{load_rules, load_rules_from_metadata, rules_from_schema, validate, Severity},
    };

    match cmd {
        ArchCommands::Extract { path, format, verbose } => {
            let annotations = extract_from_workspace_verbose(&path, verbose)?;
            if verbose {
                for ann in &annotations {
                    eprintln!("  {:?} at {}:{}", ann.kind, ann.file.display(), ann.line);
                }
            }
            let graph = ArchGraph::from_annotations(annotations);
            if verbose {
                eprintln!("Built graph with {} nodes", graph.nodes().count());
            }

            match format.as_str() {
                "json" => println!("{}", graph.to_json()?),
                "mermaid" => println!("{}", graph.to_mermaid()),
                _ => eprintln!("Unknown format: {}", format),
            }
        }

        ArchCommands::Query { query, path, format } => {
            let annotations = extract_from_workspace_verbose(&path, false)?;
            let graph = ArchGraph::from_annotations(annotations);

            let q = Query::parse(&query).map_err(|e| anyhow::anyhow!(e))?;
            let result = q.execute(&graph);

            match format.as_str() {
                "ids" => {
                    for id in &result.nodes {
                        println!("{}", id);
                    }
                    eprintln!("\n{} matches", result.count);
                }
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&result.nodes)?);
                }
                "verbose" => {
                    for id in &result.nodes {
                        if let Some(node) = graph.get_node(id) {
                            println!("{}:{} - {}", node.file.display(), node.line, node.id);
                            if let Some(ref layer) = node.properties.layer {
                                println!("  layer: {}", layer);
                            }
                            if !node.properties.roles.is_empty() {
                                println!("  roles: {}", node.properties.roles.join(", "));
                            }
                            if let Some(ref thread) = node.properties.thread {
                                println!("  thread: {:?}", thread);
                            }
                            println!();
                        }
                    }
                    eprintln!("{} matches", result.count);
                }
                _ => eprintln!("Unknown format: {}", format),
            }
        }

        ArchCommands::Trace { from, to, path } => {
            let annotations = extract_from_workspace_verbose(&path, false)?;
            let graph = ArchGraph::from_annotations(annotations);

            let paths = trace_path(&graph, &from, &to).map_err(|e| anyhow::anyhow!(e))?;

            if paths.is_empty() {
                println!("No paths found from '{}' to '{}'", from, to);
            } else {
                for (i, p) in paths.iter().enumerate() {
                    println!("Path {}:", i + 1);
                    for (j, node) in p.iter().enumerate() {
                        if j > 0 {
                            println!("  ↓");
                        }
                        println!("  {}", node);
                    }
                    println!();
                }
            }
        }

        ArchCommands::Context { file, path, format } => {
            let annotations = extract_from_workspace_verbose(&path, false)?;
            let graph = ArchGraph::from_annotations(annotations);

            let file_str = file.to_string_lossy();
            let context = get_file_context(&graph, &file_str);

            match format.as_str() {
                "markdown" => println!("{}", context.to_markdown(&file_str)),
                "json" => {
                    let json = serde_json::json!({
                        "file": file_str.to_string(),
                        "layer": context.layer,
                        "roles": context.roles,
                        "thread": context.thread,
                        "qos": context.qos,
                        "produces": context.produces,
                        "consumes": context.consumes,
                        "patterns": context.patterns,
                        "constraints": context.constraints,
                    });
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
                _ => eprintln!("Unknown format: {}", format),
            }
        }

        ArchCommands::Validate { path, rules, quiet, include_schema_rules } => {
            let annotations = extract_from_workspace_verbose(&path, false)?;
            let graph = ArchGraph::from_annotations(annotations);

            let mut all_rules = if let Some(rules_path) = rules {
                let content = std::fs::read_to_string(&rules_path)?;
                load_rules(&content)?
            } else {
                match load_rules_from_metadata(&path) {
                    Ok(r) => r,
                    Err(e) => {
                        if !quiet {
                            eprintln!("Note: Could not load rules from Cargo.toml: {}", e);
                        }
                        Vec::new()
                    }
                }
            };

            if include_schema_rules {
                if let Ok(schema) = Schema::from_cargo_metadata(&path) {
                    all_rules.extend(rules_from_schema(&schema));
                }
            }

            if all_rules.is_empty() {
                if !quiet {
                    println!("No architecture rules defined.");
                    println!("Add rules to [[workspace.metadata.arch.rules]] in Cargo.toml");
                    println!("Or use --include-schema-rules to validate layer dependencies.");
                }
                return Ok(());
            }

            if !quiet {
                eprintln!("Validating {} rules...", all_rules.len());
            }

            let violations = validate(&graph, &all_rules);

            if violations.is_empty() {
                if !quiet {
                    println!("✓ No violations found");
                }
            } else {
                let errors = violations.iter().filter(|v| v.severity == Severity::Error).count();
                let warnings = violations.iter().filter(|v| v.severity == Severity::Warning).count();

                for v in &violations {
                    let prefix = match v.severity {
                        Severity::Error => "ERROR",
                        Severity::Warning => "WARNING",
                    };
                    let location = match (&v.file, v.line) {
                        (Some(f), Some(l)) => format!("{}:{}", f.display(), l),
                        (Some(f), None) => f.display().to_string(),
                        _ => "unknown".into(),
                    };
                    println!("{}: {} - {} ({})", prefix, v.rule, v.message, location);
                }

                if errors > 0 {
                    eprintln!("\n✗ {} errors, {} warnings", errors, warnings);
                    std::process::exit(1);
                } else {
                    eprintln!("\n⚠ {} warnings", warnings);
                }
            }
        }

        ArchCommands::Visualize { path, format } => {
            let annotations = extract_from_workspace_verbose(&path, false)?;
            let graph = ArchGraph::from_annotations(annotations);

            match format.as_str() {
                "mermaid" => println!("{}", graph.to_mermaid()),
                "dot" => {
                    println!("digraph arch {{");
                    println!("  rankdir=TB;");
                    println!("  node [shape=box];");

                    for node in graph.nodes() {
                        let label = node.id.split("::").last().unwrap_or(&node.id);
                        let color = node.properties.layer.as_deref()
                            .map(|l| match l {
                                "core" => "lightblue",
                                "lib" => "lightgreen",
                                "api" => "lightyellow",
                                "ui" => "lightpink",
                                "app" => "lightgray",
                                _ => "white",
                            })
                            .unwrap_or("white");
                        println!(
                            "  \"{}\" [label=\"{}\" fillcolor=\"{}\" style=filled];",
                            node.id, label, color
                        );
                    }

                    for edge in graph.edges() {
                        let style = match edge.kind {
                            EdgeKind::DependsOn => "solid",
                            EdgeKind::MessageFlow => "dashed",
                            EdgeKind::Bridge => "bold",
                            _ => "dotted",
                        };
                        println!("  \"{}\" -> \"{}\" [style={}];", edge.from, edge.to, style);
                    }

                    println!("}}");
                }
                _ => eprintln!("Unknown format: {}", format),
            }
        }

        ArchCommands::Schema { path, format } => {
            let schema = Schema::from_cargo_metadata(&path)?;

            if schema.is_empty() {
                eprintln!("No architecture schema found in Cargo.toml");
                eprintln!("Add [workspace.metadata.arch] section to configure.");
                eprintln!("Run 'rs-hack arch init' for a template.");
                return Ok(());
            }

            match format.as_str() {
                "summary" => println!("{}", schema.summary()),
                "toml" => println!("{}", schema.to_toml()?),
                "json" => println!("{}", serde_json::to_string_pretty(&schema)?),
                _ => eprintln!("Unknown format: {}", format),
            }
        }

        ArchCommands::Init { format, apply, path } => {
            match format.as_str() {
                "toml" => {
                    if apply {
                        apply_arch_init_template(&path)?;
                    } else {
                        print_arch_init_template();
                        eprintln!("\n# To apply this to your Cargo.toml, run:");
                        eprintln!("#   rs-hack arch init --apply");
                    }
                }
                "example" => print_arch_example_annotations(),
                _ => eprintln!("Unknown format: {}", format),
            }
        }

    }

    Ok(())
}

// ── `rs-hack board claim` — atomic next-ID picker + annotation writer ────

struct ClaimArgs {
    path: PathBuf,
    kind: String,
    file: PathBuf,
    title: String,
    assignee: Option<String>,
    status: Option<String>,
    phase: Option<String>,
    parent: Option<String>,
    severity: Option<String>,
    handoff: Vec<String>,
    next: Vec<String>,
    verify: Vec<String>,
    cleanup: Vec<String>,
    gotcha: Vec<String>,
    assumes: Vec<String>,
    see: Vec<String>,
    json: bool,
}

/// Holds an exclusive lock on `.yah/id.lock` for the duration of an ID claim.
/// Uses create_new so the first process wins atomically; later ones busy-wait
/// up to ~5 seconds with exponential backoff.
struct IdLock {
    path: PathBuf,
}

impl IdLock {
    fn acquire(workspace: &Path) -> Result<Self> {
        let hack_dir = workspace.join(".yah");
        std::fs::create_dir_all(&hack_dir)?;
        let path = hack_dir.join("id.lock");
        let start = std::time::Instant::now();
        let mut delay_ms = 10u64;
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut f) => {
                    use std::io::Write;
                    let _ = writeln!(
                        f,
                        "pid={} claimed_at={}",
                        std::process::id(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0)
                    );
                    return Ok(IdLock { path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if start.elapsed() > std::time::Duration::from_secs(5) {
                        anyhow::bail!(
                            "Another rs-hack process is holding {}; \
                             waited 5s. Delete the lock file if stale.",
                            path.display()
                        );
                    }
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    delay_ms = (delay_ms * 2).min(200);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

impl Drop for IdLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn handle_claim(args: ClaimArgs) -> Result<()> {
    use arch::ticket::{ItemType, TicketBoard};

    let workspace = std::fs::canonicalize(&args.path).unwrap_or(args.path.clone());
    // Resolve the target file relative to the workspace if not already absolute
    let target = if args.file.is_absolute() {
        args.file.clone()
    } else {
        workspace.join(&args.file)
    };
    if !target.exists() {
        anyhow::bail!("Target file does not exist: {}", target.display());
    }
    // Only .rs files are scanned by the extractor today — writing an
    // annotation to a non-Rust file silently drops the ticket off the board.
    // See @yah:ticket(R001-T2) in rs-hack-arch/src/extract.rs for the tracked
    // fix (AnnotationTarget::File + per-language comment prefixes).
    match target.extension().and_then(|e| e.to_str()) {
        Some("rs") => {}
        Some(ext) => anyhow::bail!(
            "board claim only supports .rs files; .{} is not scanned by the extractor \
             yet (see R001-T2 in rs-hack-arch/src/extract.rs). \
             Anchor the ticket on a .rs file (use @arch:see(...) to point at the doc).",
            ext
        ),
        None => anyhow::bail!(
            "board claim requires a file with a .rs extension; got {}",
            target.display()
        ),
    }
    let target = std::fs::canonicalize(&target).unwrap_or(target);

    // Normalize kind. `epic` is shorthand for "relay with @yah:kind(epic)".
    let kind = args.kind.to_lowercase();
    let (prefix, pad, is_relay, force_kind) = match kind.as_str() {
        "relay" | "r" => ("R", 3, true, None),
        "epic" | "e" => ("R", 3, true, Some("epic")),
        "feature" | "f" => ("F", 2, false, None),
        "bug" | "b" => ("B", 2, false, None),
        "task" | "t" => ("T", 2, false, None),
        other => anyhow::bail!(
            "Unknown kind '{}'. Expected: relay | epic | feature | bug | task",
            other
        ),
    };

    // Non-relay kinds must live under a relay. Bare T01 / F02 / B01 IDs
    // collide with the compound ID numbering space when scanners filter by
    // `T` prefix, and their per-id shards (`.yah/events/T01.jsonl`) get
    // scrambled when another orphan T01 lands in a sibling workspace.
    // Relays coordinate work; tasks/features/bugs are always *inside* one.
    if !is_relay && args.parent.is_none() {
        anyhow::bail!(
            "{} requires a parent relay.\n\n\
             Pick one:\n  \
             • Attach to an existing relay:   rs-hack board {} --kind {} --parent R<NNN> --file {} --title '...'\n  \
             • Create a new relay first:      rs-hack board open --kind relay --file {} --title '...'   # prints the R-id\n\n\
             Bare {}-numbers (T01/F01/B01) are no longer allocated — they collide with compound sub-ticket numbering and shard routing.",
            kind,
            if args.status.as_deref() == Some("open") { "open" } else { "claim" },
            kind,
            args.file.display(),
            args.file.display(),
            prefix,
        );
    }

    // Acquire lock → scan → pick → write → release (lock auto-released on drop).
    let _lock = IdLock::acquire(&workspace)?;

    let annotations = arch::extract::extract_from_workspace(&workspace)?;
    let board = TicketBoard::from_annotations(&annotations);

    // R002 P1: union the local IDs with whatever IDs are visible in sibling
    // workspaces (separate clones, git worktrees, SSH peers). Per-sibling
    // failures are warnings, not hard errors — race resolution is P5's job
    // (`board rebase-ids`).
    let mut all_ids: std::collections::BTreeSet<String> =
        board.tickets.iter().map(|t| t.id.clone()).collect();
    match worktrees::union_sibling_ids(&workspace) {
        Ok(sib_ids) => all_ids.extend(sib_ids),
        Err(e) => eprintln!("warning: sibling enumeration failed: {}", e),
    }

    // Event shards are append-only — `.yah/events/<shard>.jsonl` filenames
    // are a permanent floor of every relay-id ever allocated locally. Seeding
    // these prevents reissuing an R-id whose bare relay annotation got
    // clobbered (the relay's children may also be gone, leaving nothing in
    // source to mark the slot as taken). Compound sub-tickets share their
    // parent's shard, so this set is exactly the set of bare R-ids.
    let events_dir = workspace.join(".yah").join("events");
    if let Ok(entries) = std::fs::read_dir(&events_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if let Some(stem) = name.strip_suffix(".jsonl") {
                    all_ids.insert(stem.to_string());
                }
            }
        }
    }

    // If --parent is set and this isn't itself a relay, allocate a compound
    // sub-ticket ID of the form `<parent>-<L><n>` where <L> reflects --kind
    // (B = bug, F = feature, T = task). The counter is unified across kinds
    // within a parent — opening a bug, then a feature, then a task under
    // R011 yields R011-B1, R011-F2, R011-T3. This makes the letter a kind
    // badge while preserving a single sequential order of work per relay.
    let id = if !is_relay {
        // Orphan guard above rejected `--parent`-less non-relay kinds; this
        // branch is only reached with a concrete parent.
        let parent_id = args.parent.as_ref().expect("orphan guard");
        let compound_prefix = format!("{}-", parent_id);
        let next_sub: u32 = all_ids
            .iter()
            .filter_map(|id| {
                let rest = id.strip_prefix(&compound_prefix)?;
                let mut chars = rest.chars();
                let letter = chars.next()?;
                if !matches!(letter, 'B' | 'F' | 'T') {
                    return None;
                }
                chars.as_str().parse::<u32>().ok()
            })
            .max()
            .map(|n| n + 1)
            .unwrap_or(1);
        format!("{}-{}{}", parent_id, prefix, next_sub)
    } else {
        // Relay: bare R-number. A compound id (`R007-T1`) implies its parent
        // (`R007`) is taken even when the bare relay annotation has been
        // clobbered out of source — without this, a fresh `board open` could
        // reissue R007 and the scanner would Frankenstein the surviving
        // children onto the new relay. Take the parent prefix of every id.
        let existing_max: u32 = all_ids
            .iter()
            .filter_map(|id| {
                let bare = id.split('-').next()?;
                if !bare.starts_with(prefix) {
                    return None;
                }
                bare[prefix.len()..].parse::<u32>().ok()
            })
            .max()
            .unwrap_or(0);
        format!("{}{:0pad$}", prefix, existing_max + 1, pad = pad)
    };

    // Build the @yah: annotation lines.
    // Epics: status is derived from children, so no default line is written
    // (leave `status` blank so the annotation omits it entirely — an explicit
    // user-provided --status still gets written for backwards compat).
    let default_status = if force_kind == Some("epic") {
        "" // epics compute their status from children
    } else if is_relay {
        "handoff"
    } else {
        "in-progress"
    };
    let status = args.status.as_deref().unwrap_or(default_status);
    let status_opt = if status.is_empty() { None } else { Some(status) };
    let ann = build_annotation_block(
        is_relay,
        &id,
        &args.title,
        status_opt,
        force_kind,
        args.assignee.as_deref(),
        args.phase.as_deref(),
        args.parent.as_deref(),
        args.severity.as_deref(),
        &args.handoff,
        &args.next,
        &args.verify,
        &args.cleanup,
        &args.gotcha,
        &args.assumes,
        &args.see,
    );

    let warnings = arch::smell::write_time_warnings(
        None,
        &args.handoff,
        &args.next,
    );
    for w in &warnings {
        eprintln!("⚠ {}", w);
    }

    // Insert into target file: append to leading //! block if present, else prepend.
    let original = std::fs::read_to_string(&target)?;
    let (new_content, line) = insert_module_doc_block(&original, &ann);
    std::fs::write(&target, new_content)?;

    // Output: workspace-relative path when possible
    let rel_file: PathBuf = target
        .strip_prefix(&workspace)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| target.clone());
    if args.json {
        let out = serde_json::json!({
            "id": id,
            "file": rel_file.display().to_string(),
            "line": line,
        });
        println!("{}", out);
    } else {
        println!("{}", id);
        eprintln!("{}:{}", rel_file.display(), line);
    }
    Ok(())
}

// ── `rs-hack board claim <ID>` — flip an Open ticket into Active ────────

fn current_agent_id() -> String {
    std::env::var("CLAUDE_AGENT")
        .or_else(|_| std::env::var("HACK_AGENT"))
        .unwrap_or_else(|_| "agent:claude".to_string())
}

// ── `rs-hack board inflight` — plan-time discovery view (Rule10) ────────────

fn handle_inflight(path: &Path, format: &str, include_epics: bool) -> Result<()> {
    use arch::ticket::{TicketBoard, TicketStatus};

    let workspace = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let annotations = arch::extract::extract_from_workspace(&workspace)?;
    let board = TicketBoard::from_annotations(&annotations);

    // Bucket into the three in-flight columns. Everything else (review, done,
    // unrecognized) is dropped — by definition not in flight.
    let mut active = Vec::new();
    let mut handoff = Vec::new();
    let mut open = Vec::new();
    for t in &board.tickets {
        if t.is_epic && !include_epics {
            continue;
        }
        match t.status {
            TicketStatus::Claimed | TicketStatus::InProgress => active.push(t),
            TicketStatus::Handoff => handoff.push(t),
            TicketStatus::Open => open.push(t),
            TicketStatus::Review | TicketStatus::Done => {}
        }
    }
    active.sort_by(|a, b| a.id.cmp(&b.id));
    handoff.sort_by(|a, b| a.id.cmp(&b.id));
    open.sort_by(|a, b| a.id.cmp(&b.id));

    match format {
        "json" => {
            let payload = serde_json::json!({
                "active": active.iter().map(inflight_json_row).collect::<Vec<_>>(),
                "handoff": handoff.iter().map(inflight_json_row).collect::<Vec<_>>(),
                "open": open.iter().map(inflight_json_row).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        "grep" => {
            for t in &active {
                println!("{}", inflight_grep_row("active", t));
            }
            for t in &handoff {
                println!("{}", inflight_grep_row("handoff", t));
            }
            for t in &open {
                println!("{}", inflight_grep_row("open", t));
            }
        }
        _ => {
            let total = active.len() + handoff.len() + open.len();
            println!("# In-flight — {} relay{}/tickets", total, if total == 1 { "" } else { "s" });
            println!();
            render_inflight_column("ACTIVE", &active);
            render_inflight_column("HANDOFF", &handoff);
            render_inflight_column("OPEN", &open);
            if total == 0 {
                println!("_No relays or tickets currently in flight._");
                println!();
            }
            println!("---");
            println!();
            println!(
                "Scanning for overlap before `/refine` or `rs-hack board open --kind relay` \
                 (Rule10). If your planned work matches an existing relay's purpose, claim it, \
                 open a sub-ticket under it (`--parent R<n>`), or reference it in your arch doc."
            );
        }
    }
    Ok(())
}

fn render_inflight_column(header: &str, items: &[&arch::ticket::Ticket]) {
    if items.is_empty() {
        return;
    }
    println!("## {} ({})", header, items.len());
    println!();
    // Calculate ID / assignee column widths for alignment.
    let id_w = items.iter().map(|t| t.id.len()).max().unwrap_or(4);
    let assignee_w = items
        .iter()
        .map(|t| t.assignee.as_deref().unwrap_or("—").len())
        .max()
        .unwrap_or(1);
    for t in items {
        let assignee = t.assignee.as_deref().unwrap_or("—");
        let parent_tag = t
            .parent
            .as_deref()
            .map(|p| format!("  (parent: {})", p))
            .unwrap_or_default();
        let see_tag = if t.see_also.is_empty() {
            String::new()
        } else {
            format!("  → {}", t.see_also.join(", "))
        };
        println!(
            "  {:<id_w$}  {:<assignee_w$}  {}{}{}",
            t.id,
            assignee,
            t.title,
            parent_tag,
            see_tag,
            id_w = id_w,
            assignee_w = assignee_w
        );
    }
    println!();
}

fn inflight_grep_row(column: &str, t: &arch::ticket::Ticket) -> String {
    let assignee = t.assignee.as_deref().unwrap_or("—");
    let see = if t.see_also.is_empty() {
        String::new()
    } else {
        format!(" see={}", t.see_also.join(","))
    };
    let parent = t
        .parent
        .as_deref()
        .map(|p| format!(" parent={}", p))
        .unwrap_or_default();
    format!("{} {} {} {:?}{}{}", column, t.id, assignee, t.title, parent, see)
}

fn inflight_json_row(t: &&arch::ticket::Ticket) -> serde_json::Value {
    serde_json::json!({
        "id": t.id,
        "title": t.title,
        "assignee": t.assignee,
        "parent": t.parent,
        "phase": t.phase,
        "see_also": t.see_also,
        "is_epic": t.is_epic,
    })
}

fn handle_claim_existing(id: &str, path: &Path, assignee: &str) -> Result<()> {
    use arch::ticket::{TicketBoard, TicketStatus};

    let workspace = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let annotations = arch::extract::extract_from_workspace(&workspace)?;
    let board = TicketBoard::from_annotations(&annotations);
    let ticket = board
        .tickets
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| anyhow::anyhow!("Ticket '{}' not found in workspace", id))?;

    if ticket.status != TicketStatus::Open {
        anyhow::bail!(
            "Ticket '{}' is in status '{}', not 'open'. Use `rs-hack board move {} active` \
             to transition from handoff, or edit the annotation directly.",
            id,
            ticket_status_str(&ticket.status),
            id
        );
    }

    let target_file = if ticket.file.is_absolute() {
        ticket.file.clone()
    } else {
        workspace.join(&ticket.file)
    };
    let content = std::fs::read_to_string(&target_file)
        .with_context(|| format!("Failed to read {}", target_file.display()))?;
    let block = locate_ticket_block(&content, id).ok_or_else(|| {
        anyhow::anyhow!(
            "Could not locate @yah:ticket/@yah:relay({}, ...) decl in {}",
            id,
            target_file.display()
        )
    })?;

    let mut new_content =
        set_or_insert_annotation(&content, &block, "status", "in-progress", true);
    let block2 = locate_ticket_block(&new_content, id).unwrap();
    new_content = set_or_insert_annotation(&new_content, &block2, "assignee", assignee, true);
    std::fs::write(&target_file, &new_content)
        .with_context(|| format!("Failed to write {}", target_file.display()))?;

    let rel_file: PathBuf = target_file
        .strip_prefix(&workspace)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| target_file.clone());
    eprintln!(
        "{}:{}  open → in-progress  (assignee={})",
        rel_file.display(),
        block.decl_line,
        assignee
    );
    println!("{}", id);
    Ok(())
}

// ── `rs-hack board move` — column transition + payload append ───────────

struct MoveArgs {
    id: String,
    column: String,
    path: PathBuf,
    assignee: Option<String>,
    handoff: Vec<String>,
    next: Vec<String>,
    verify: Vec<String>,
    cleanup: Vec<String>,
    gotcha: Vec<String>,
    assumes: Vec<String>,
}

/// Column buckets surfaced on the UI → canonical status strings. Kept in
/// sync with `BUCKET_TO_STATUS` in hack-board/src/server.ts.
fn bucket_to_status(bucket: &str) -> Option<&'static str> {
    match bucket {
        "open" => Some("open"),
        "active" => Some("in-progress"),
        "handoff" => Some("handoff"),
        "review" => Some("review"),
        _ => None,
    }
}

/// Stringify a parsed TicketStatus back to the wire form used in @yah:status(...).
fn ticket_status_str(s: &arch::ticket::TicketStatus) -> &'static str {
    use arch::ticket::TicketStatus as S;
    match s {
        S::Open => "open",
        S::Claimed => "claimed",
        S::InProgress => "in-progress",
        S::Handoff => "handoff",
        S::Review => "review",
        S::Done => "done",
    }
}

/// Canonical status → bucket. Kept in sync with `STATUS_TO_BUCKET` in
/// hack-board/src/server.ts.
fn status_to_bucket(status: &str) -> &'static str {
    match status {
        "open" => "open",
        "claimed" | "in-progress" => "active",
        "handoff" => "handoff",
        "review" | "done" => "review",
        _ => "",
    }
}

/// Allowed column-to-column transitions. Mirror of `TRANSITIONS` in the
/// hack-board server.
fn allowed_transitions(from: &str) -> &'static [&'static str] {
    match from {
        "open" => &["active"],
        "active" => &["open", "handoff", "review"],
        "handoff" => &["active", "review"],
        "review" => &["handoff"],
        _ => &[],
    }
}

fn handle_move(args: MoveArgs) -> Result<()> {
    use arch::ticket::TicketBoard;

    let workspace = std::fs::canonicalize(&args.path).unwrap_or(args.path.clone());

    let to_bucket = args.column.to_lowercase();
    let new_status = bucket_to_status(&to_bucket).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown target column '{}'. Expected: open | active | handoff | review",
            args.column
        )
    })?;

    let annotations = arch::extract::extract_from_workspace(&workspace)?;
    let board = TicketBoard::from_annotations(&annotations);
    let ticket = board
        .tickets
        .iter()
        .find(|t| t.id == args.id)
        .ok_or_else(|| anyhow::anyhow!("Ticket '{}' not found in workspace", args.id))?;

    if ticket.is_epic {
        anyhow::bail!(
            "'{}' is an epic; its status is derived from children and cannot be set directly.",
            args.id
        );
    }

    let from_status_str = ticket_status_str(&ticket.status);
    let from_bucket = status_to_bucket(from_status_str);
    if from_bucket.is_empty() {
        anyhow::bail!(
            "Ticket '{}' has unrecognized status '{}' — cannot move.",
            args.id,
            from_status_str
        );
    }

    if from_bucket == to_bucket {
        // Still allow payload append even when the column doesn't change.
        if args.handoff.is_empty()
            && args.next.is_empty()
            && args.verify.is_empty()
            && args.cleanup.is_empty()
            && args.gotcha.is_empty()
            && args.assumes.is_empty()
            && args.assignee.is_none()
        {
            eprintln!("{} already in column '{}' — noop", args.id, to_bucket);
            println!("{}", args.id);
            return Ok(());
        }
    } else {
        let allowed = allowed_transitions(from_bucket);
        if !allowed.contains(&to_bucket.as_str()) {
            anyhow::bail!(
                "Transition {} → {} is not allowed (from {}). Allowed targets: {:?}",
                from_bucket,
                to_bucket,
                from_status_str,
                allowed
            );
        }
    }

    let target_file = if ticket.file.is_absolute() {
        ticket.file.clone()
    } else {
        workspace.join(&ticket.file)
    };
    let content = std::fs::read_to_string(&target_file)
        .with_context(|| format!("Failed to read {}", target_file.display()))?;

    let block = locate_ticket_block(&content, &args.id).ok_or_else(|| {
        anyhow::anyhow!(
            "Could not locate @yah:ticket/@yah:relay({}, ...) decl in {}",
            args.id,
            target_file.display()
        )
    })?;

    let mut new_content = content.clone();
    let mut block = block;

    if from_bucket != to_bucket {
        new_content = set_or_insert_annotation(
            &new_content,
            &block,
            "status",
            new_status,
            true,
        );
        // Re-locate — line numbers may have shifted by one insert.
        block = locate_ticket_block(&new_content, &args.id).unwrap();
    }

    if let Some(ref who) = args.assignee {
        new_content = set_or_insert_annotation(
            &new_content,
            &block,
            "assignee",
            who,
            true,
        );
        block = locate_ticket_block(&new_content, &args.id).unwrap();
    }

    let mut appended = Vec::<String>::new();
    for h in &args.handoff {
        appended.push(format!("//! @yah:handoff({:?})", h));
    }
    for n in &args.next {
        appended.push(format!("//! @yah:next({:?})", n));
    }
    for v in &args.verify {
        appended.push(format!("//! @yah:verify({:?})", v));
    }
    for c in &args.cleanup {
        appended.push(format!("//! @yah:cleanup({:?})", c));
    }
    for g in &args.gotcha {
        appended.push(format!("//! @yah:gotcha({:?})", g));
    }
    for a in &args.assumes {
        appended.push(format!("//! @yah:assumes({:?})", a));
    }
    if !appended.is_empty() {
        new_content = append_lines_to_block(&new_content, &block, &appended);
    }

    let warnings = arch::smell::write_time_warnings(
        Some(ticket),
        &args.handoff,
        &args.next,
    );
    for w in &warnings {
        eprintln!("⚠ {}", w);
    }

    std::fs::write(&target_file, &new_content)
        .with_context(|| format!("Failed to write {}", target_file.display()))?;

    let rel_file: PathBuf = target_file
        .strip_prefix(&workspace)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| target_file.clone());
    eprintln!(
        "{}:{}  {} → {} (status: {})",
        rel_file.display(),
        block.decl_line,
        from_status_str,
        new_status,
        if from_bucket == to_bucket { "unchanged" } else { "rewritten" }
    );
    println!("{}", args.id);
    Ok(())
}

/// Strip `@yah:` annotation lines from source and append an `archived`
/// event to the shard. Same effect as the kanban UI's archive button —
/// available as a CLI verb so an agent finishing a review can archive
/// without needing the server's port.
fn handle_archive(id: &str, path: &PathBuf) -> Result<()> {
    use arch::ticket::TicketBoard;

    let workspace = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
    let annotations = arch::extract::extract_from_workspace(&workspace)?;
    let board = TicketBoard::from_annotations(&annotations);

    let ticket = board
        .tickets
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| {
            // Fall through to archive shard for a clearer message — if the
            // ticket is already archived, say so explicitly instead of just
            // "not found".
            if arch::archive::lookup(&workspace, id).is_some() {
                anyhow::anyhow!(
                    "Ticket '{}' is already archived (snapshot in .yah/events/). \
                     Run `rs-hack board show {}` to see the last-known state.",
                    id, id
                )
            } else {
                anyhow::anyhow!("Ticket '{}' not found in workspace", id)
            }
        })?;

    let from_status = ticket_status_str(&ticket.status);
    if from_status == "claimed" || from_status == "in-progress" {
        anyhow::bail!(
            "Cannot archive '{}' — ticket is {}. Move it to review or handoff first \
             (`rs-hack board move {} review`). Archive is for terminal states only.",
            id, from_status, id
        );
    }

    if ticket.is_epic {
        let live_children: Vec<&str> = board
            .tickets
            .iter()
            .filter(|t| t.parent.as_deref() == Some(id))
            .map(|t| t.id.as_str())
            .collect();
        if !live_children.is_empty() {
            anyhow::bail!(
                "Epic '{}' has {} live child relay(s): {}. Archive each child first.",
                id,
                live_children.len(),
                live_children.join(", ")
            );
        }
    }

    let target_file = if ticket.file.is_absolute() {
        ticket.file.clone()
    } else {
        workspace.join(&ticket.file)
    };
    let content = std::fs::read_to_string(&target_file)
        .with_context(|| format!("Failed to read {}", target_file.display()))?;

    let block = locate_ticket_block(&content, id).ok_or_else(|| {
        anyhow::anyhow!(
            "Could not locate @yah:ticket/@yah:relay({}, ...) decl in {}",
            id, target_file.display()
        )
    })?;

    // Strip every `//[!/] @yah:` line within the block (1-indexed inclusive).
    // Leaves `@arch:` lines and surrounding non-hack doc lines untouched —
    // matches stripHackAnnotations in hack-board/src/server.ts.
    let mut removed: Vec<String> = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    let is_hack_line = |l: &str| -> bool {
        let t = l.trim_start();
        if !(t.starts_with("//!") || t.starts_with("///")) {
            return false;
        }
        t[3..].trim_start().starts_with("@yah:")
    };
    for (i, line) in content.split('\n').enumerate() {
        let line_num = i + 1;
        if line_num >= block.start_line && line_num <= block.end_line && is_hack_line(line) {
            removed.push(line.to_string());
        } else {
            kept.push(line.to_string());
        }
    }
    if removed.is_empty() {
        anyhow::bail!(
            "No @yah: annotations found at {}:{}",
            target_file.display(),
            block.decl_line
        );
    }
    let new_content = kept.join("\n");
    std::fs::write(&target_file, &new_content)
        .with_context(|| format!("Failed to write {}", target_file.display()))?;

    // Choose shard: parent's if it's a sub-ticket, else the ticket's own.
    let shard = ticket
        .parent
        .as_deref()
        .map(arch::archive::shard_for)
        .unwrap_or_else(|| arch::archive::shard_for(&ticket.id));

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let event = serde_json::json!({
        "t": now,
        "type": "archived",
        "id": ticket.id,
        "ticket": ticket,
        "sourceLines": removed,
        "file": ticket.file,
        "line": block.decl_line,
    });
    arch::archive::append_event(&workspace, shard, &event)
        .with_context(|| format!("Failed to append archived event to .yah/events/{shard}.jsonl"))?;

    let rel_file: PathBuf = target_file
        .strip_prefix(&workspace)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| target_file.clone());
    eprintln!(
        "{}:{}  archived ({} line(s) stripped, snapshot in .yah/events/{}.jsonl)",
        rel_file.display(),
        block.decl_line,
        removed.len(),
        shard
    );
    eprintln!("Inspect later: rs-hack board show {}", ticket.id);
    println!("{}", ticket.id);
    Ok(())
}

#[derive(Debug, Clone)]
struct TicketBlock {
    /// 1-indexed: the `@yah:ticket(ID,...)` or `@yah:relay(ID,...)` line.
    decl_line: usize,
    /// 1-indexed: first line of the contiguous @hack/@arch annotation run.
    start_line: usize,
    /// 1-indexed: last line of that run.
    end_line: usize,
}

/// Find the contiguous run of `//! @yah:...` / `//! @arch:...` lines that
/// belongs to the ticket or relay with the given ID. Runs are separated by
/// blank `//!` lines (as written by `insert_module_doc_block`), by a
/// non-doc-comment line, or by another `@yah:ticket(...)` / `@yah:relay(...)`
/// declaration.
fn locate_ticket_block(content: &str, id: &str) -> Option<TicketBlock> {
    let lines: Vec<&str> = content.split('\n').collect();

    let decl_ticket = format!("@yah:ticket({},", id);
    let decl_relay = format!("@yah:relay({},", id);
    let decl_ticket_sp = format!("@yah:ticket({} ,", id);
    let decl_relay_sp = format!("@yah:relay({} ,", id);

    let is_this_decl = |line: &str| -> bool {
        line.contains(&decl_ticket)
            || line.contains(&decl_relay)
            || line.contains(&decl_ticket_sp)
            || line.contains(&decl_relay_sp)
    };
    let is_any_decl = |line: &str| -> bool {
        line.contains("@yah:ticket(") || line.contains("@yah:relay(")
    };

    let is_doc = |i: usize| -> bool {
        let t = lines[i].trim_start();
        t.starts_with("//!") || t.starts_with("///")
    };
    let is_blank_doc = |i: usize| -> bool {
        let t = lines[i].trim();
        t == "//!" || t == "///"
    };
    let is_yah_or_arch = |i: usize| -> bool {
        let t = lines[i].trim_start();
        (t.starts_with("//!") || t.starts_with("///"))
            && (t.contains("@yah:") || t.contains("@arch:"))
    };

    let decl_idx = (0..lines.len()).find(|&i| is_doc(i) && is_this_decl(lines[i]))?;

    // Walk up
    let mut start = decl_idx;
    while start > 0 {
        let prev = start - 1;
        if !is_doc(prev) || is_blank_doc(prev) {
            break;
        }
        if prev != decl_idx && is_yah_or_arch(prev) && is_any_decl(lines[prev]) {
            break;
        }
        if !is_yah_or_arch(prev) {
            // Non-annotation doc text — stop.
            break;
        }
        start = prev;
    }

    // Walk down
    let mut end = decl_idx;
    while end + 1 < lines.len() {
        let nxt = end + 1;
        if !is_doc(nxt) || is_blank_doc(nxt) {
            break;
        }
        if is_yah_or_arch(nxt) && is_any_decl(lines[nxt]) {
            break;
        }
        if !is_yah_or_arch(nxt) {
            break;
        }
        end = nxt;
    }

    Some(TicketBlock {
        decl_line: decl_idx + 1,
        start_line: start + 1,
        end_line: end + 1,
    })
}

/// Rewrite `@yah:<key>(<value>)` inside `block` if present; otherwise insert
/// it immediately after the declaration line. When `replace` is false and the
/// key already exists, the value is left alone.
fn set_or_insert_annotation(
    content: &str,
    block: &TicketBlock,
    key: &str,
    value: &str,
    replace: bool,
) -> String {
    let mut lines: Vec<String> = content.split('\n').map(|s| s.to_string()).collect();
    let needle = format!("@yah:{}(", key);

    for i in (block.start_line - 1)..block.end_line {
        let trimmed = lines[i].trim_start();
        let prefix_end = lines[i].len() - trimmed.len();
        let indent = &lines[i][..prefix_end];
        let rest_after_sigil = if let Some(r) = trimmed.strip_prefix("//!") {
            Some(("//!", r))
        } else if let Some(r) = trimmed.strip_prefix("///") {
            Some(("///", r))
        } else {
            None
        };
        let Some((sigil, after)) = rest_after_sigil else {
            continue;
        };
        if after.trim_start().starts_with(&needle) {
            if replace {
                lines[i] = format!("{}{} @yah:{}({})", indent, sigil, key, value);
            }
            return lines.join("\n");
        }
    }

    // Not present — insert immediately after decl_line, preserving the decl's
    // indentation / doc-comment sigil.
    let decl_idx = block.decl_line - 1;
    let prefix = extract_doc_prefix(&lines[decl_idx]);
    let new_line = format!("{} @yah:{}({})", prefix, key, value);
    lines.insert(decl_idx + 1, new_line);
    lines.join("\n")
}

/// Append raw annotation lines to the end of a ticket's block. The lines are
/// assumed to be pre-formatted with `//!` — this function adjusts the sigil
/// to match the block's existing style (`//!` vs `///`) before inserting.
fn append_lines_to_block(
    content: &str,
    block: &TicketBlock,
    new_lines_raw: &[String],
) -> String {
    let mut lines: Vec<String> = content.split('\n').map(|s| s.to_string()).collect();
    let decl_idx = block.decl_line - 1;
    let prefix = extract_doc_prefix(&lines[decl_idx]);
    let adjusted: Vec<String> = new_lines_raw
        .iter()
        .map(|l| {
            // Strip leading `//!` / `///` and re-attach the block's prefix.
            let trimmed = l.trim_start();
            let rest = if let Some(r) = trimmed.strip_prefix("//!") {
                r
            } else if let Some(r) = trimmed.strip_prefix("///") {
                r
            } else {
                trimmed
            };
            format!("{}{}", prefix, rest)
        })
        .collect();
    let insert_at = block.end_line; // 1-indexed end → 0-indexed insert position
    for (off, l) in adjusted.into_iter().enumerate() {
        lines.insert(insert_at + off, l);
    }
    lines.join("\n")
}

fn extract_doc_prefix(line: &str) -> String {
    // Preserve leading whitespace + `//!` or `///`.
    let ws_end = line.find(|c: char| !c.is_whitespace()).unwrap_or(0);
    let ws = &line[..ws_end];
    let rest = &line[ws_end..];
    if rest.starts_with("//!") {
        format!("{}//!", ws)
    } else if rest.starts_with("///") {
        format!("{}///", ws)
    } else {
        format!("{}//!", ws)
    }
}

#[allow(clippy::too_many_arguments)]
fn build_annotation_block(
    is_relay: bool,
    id: &str,
    title: &str,
    status: Option<&str>,
    kind_override: Option<&str>,
    assignee: Option<&str>,
    phase: Option<&str>,
    parent: Option<&str>,
    severity: Option<&str>,
    handoff: &[String],
    next: &[String],
    verify: &[String],
    cleanup: &[String],
    gotcha: &[String],
    assumes: &[String],
    see: &[String],
) -> String {
    let mut lines = Vec::<String>::new();
    let head = if is_relay { "relay" } else { "ticket" };
    lines.push(format!(
        "//! @yah:{}({}, {:?})",
        head, id, title
    ));
    if let Some(k) = kind_override {
        lines.push(format!("//! @yah:kind({})", k));
    }
    if let Some(s) = status {
        lines.push(format!("//! @yah:status({})", s));
    }
    if let Some(a) = assignee {
        lines.push(format!("//! @yah:assignee({})", a));
    }
    if let Some(p) = phase {
        lines.push(format!("//! @yah:phase({})", p));
    }
    if let Some(p) = parent {
        lines.push(format!("//! @yah:parent({})", p));
    }
    if let Some(s) = severity {
        lines.push(format!("//! @yah:severity({})", s));
    }
    for h in handoff {
        lines.push(format!("//! @yah:handoff({:?})", h));
    }
    for n in next {
        lines.push(format!("//! @yah:next({:?})", n));
    }
    for v in verify {
        lines.push(format!("//! @yah:verify({:?})", v));
    }
    for c in cleanup {
        lines.push(format!("//! @yah:cleanup({:?})", c));
    }
    for g in gotcha {
        lines.push(format!("//! @yah:gotcha({:?})", g));
    }
    for a in assumes {
        lines.push(format!("//! @yah:assumes({:?})", a));
    }
    for s in see {
        lines.push(format!("//! @arch:see({})", s));
    }
    lines.join("\n")
}

/// Insert `block` into `content` as a module-level doc comment. If the file
/// already starts with `//!` lines, append the block to that run (separated by
/// a blank `//!` line). Otherwise insert at the very top.
/// Returns the new content and the 1-indexed line number of the block's first
/// line in the new content.
fn insert_module_doc_block(content: &str, block: &str) -> (String, usize) {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut head_end = 0usize;
    while head_end < lines.len() {
        let l = lines[head_end];
        let trimmed = l.trim_start();
        if trimmed.starts_with("//!") {
            head_end += 1;
        } else {
            break;
        }
    }

    if head_end > 0 {
        // Append to existing `//!` block.
        let mut new_lines: Vec<String> = lines[..head_end].iter().map(|s| s.to_string()).collect();
        // Separator between existing block and new annotations
        new_lines.push("//!".to_string());
        let insert_start_line = new_lines.len() + 1; // 1-indexed
        for l in block.split('\n') {
            new_lines.push(l.to_string());
        }
        for l in &lines[head_end..] {
            new_lines.push((*l).to_string());
        }
        (new_lines.join("\n"), insert_start_line)
    } else {
        // No leading doc block — prepend.
        let mut out = String::new();
        out.push_str(block);
        out.push_str("\n\n");
        out.push_str(content);
        (out, 1)
    }
}

fn apply_arch_init_template(path: &Path) -> Result<()> {
    // Find workspace Cargo.toml
    let cargo_toml = if path.join("Cargo.toml").exists() {
        path.join("Cargo.toml")
    } else {
        // Try to find it via cargo metadata
        let output = std::process::Command::new("cargo")
            .args(["locate-project", "--workspace", "--message-format=plain"])
            .current_dir(path)
            .output()
            .context("Failed to run cargo locate-project")?;
        PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
    };

    if !cargo_toml.exists() {
        bail!("Could not find Cargo.toml at {:?}", cargo_toml);
    }

    // Read existing content
    let content = std::fs::read_to_string(&cargo_toml)
        .context("Failed to read Cargo.toml")?;

    // Check if arch metadata already exists
    if content.contains("[workspace.metadata.arch]") {
        eprintln!("Architecture metadata already exists in {}", cargo_toml.display());
        eprintln!("Edit manually or remove existing section first.");
        return Ok(());
    }

    // Append the template
    let template = r#"
# Architecture Knowledge Graph Configuration
# See: rs-hack arch --help
[workspace.metadata.arch]

# Define architectural layers (customize for your project)
[workspace.metadata.arch.layers]
core = { description = "Core library with no external dependencies", allowed_dependencies = [] }
lib = { description = "Library layer", allowed_dependencies = ["core"] }
app = { description = "Application entry points", allowed_dependencies = ["core", "lib"] }

# Define roles for components
[workspace.metadata.arch.roles]
compiler = "Transforms source to executable form"
runtime = "Manages execution state"
transport = "Routes messages between components"

# Define validation rules
[[workspace.metadata.arch.rules]]
name = "core-independence"
description = "Core layer cannot depend on higher layers"
severity = "error"
type = "layer_dependency"
layer = "core"
allowed = []
"#;

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&cargo_toml)
        .context("Failed to open Cargo.toml for appending")?;

    use std::io::Write;
    writeln!(file, "{}", template)?;

    eprintln!("Added [workspace.metadata.arch] to {}", cargo_toml.display());
    eprintln!("Next steps:");
    eprintln!("  1. Edit the layers/roles to match your architecture");
    eprintln!("  2. Add @arch: annotations to your source files");
    eprintln!("  3. Run 'rs-hack arch extract' to build the graph");
    Ok(())
}

fn print_arch_init_template() {
    println!(r#"# Add this to your workspace Cargo.toml

[workspace.metadata.arch]

# Define architectural layers
[workspace.metadata.arch.layers]
core = {{ description = "Core library with no external dependencies", allowed_dependencies = [] }}
lib = {{ description = "Library layer", allowed_dependencies = ["core"] }}
api = {{ description = "API/service layer", allowed_dependencies = ["core", "lib"] }}
app = {{ description = "Application entry points", allowed_dependencies = ["core", "lib", "api"] }}

# Define roles for components
[workspace.metadata.arch.roles]
compiler = "Transforms source to executable form"
runtime = "Manages execution state"
transport = "Routes messages between components"
storage = "Persists and retrieves data"

# Define thread contexts (optional)
[workspace.metadata.arch.threads]
main = {{ priority = "normal", description = "Main thread" }}
worker = {{ priority = "normal", description = "Background worker threads" }}

# Define validation rules
[[workspace.metadata.arch.rules]]
name = "core-independence"
description = "Core layer cannot depend on higher layers"
severity = "error"
type = "layer_dependency"
layer = "core"
allowed = []
"#);
}

fn print_arch_example_annotations() {
    println!(r#"# Example @arch: annotations for Rust source files

## Module-level (at top of file with //! comments):

//! @arch:layer(core)
//! @arch:role(runtime)

## Struct-level:

/// @arch:layer(lib)
/// @arch:role(storage)
/// @arch:produces(event:Created, event:Updated)
pub struct Repository {{ ... }}

## Function-level:

/// @arch:thread(worker)
/// @arch:consumes(command:Create)
pub fn process_command(cmd: Command) {{ ... }}

## Available annotations:

@arch:layer(name)              - Architectural layer
@arch:role(name)               - Component role
@arch:thread(name)             - Thread context
@arch:produces(type:name, ...) - Messages this component produces
@arch:consumes(type:name, ...) - Messages this component consumes
@arch:depends_on(target)       - Explicit dependency
@arch:pattern(name)            - Design pattern in use
@arch:gateway                  - Protocol translation point
"#);
}

/// Expand semantic kind to list of specific node types
fn expand_kind_to_node_types(kind: &str) -> Vec<&'static str> {
    match kind {
        "struct" => vec!["struct", "struct-literal"],
        "function" => vec!["function", "function-call", "method-call", "impl-method", "trait-method"],
        "enum" => vec!["enum", "enum-usage"],
        "match" => vec!["match-arm"],
        "identifier" => vec!["identifier"],
        "type" => vec!["type-ref", "type-alias"],
        "macro" => vec!["macro-call"],
        "const" => vec!["const", "static"],
        "trait" => vec!["trait"],
        "mod" => vec!["mod"],
        "use" => vec!["use"],
        _ => vec![],
    }
}

fn collect_rust_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    collect_rust_files_with_exclusions(paths, &[])
}

fn collect_rust_files_with_exclusions(paths: &[PathBuf], exclude_patterns: &[String]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for path in paths {
        let path_str = path.to_string_lossy();

        // Check if path contains glob pattern characters
        if path_str.contains('*') || path_str.contains('?') || path_str.contains('[') {
            // Use glob pattern matching
            for entry in glob(&path_str)
                .context("Failed to parse glob pattern")?
            {
                match entry {
                    Ok(file_path) => {
                        if file_path.is_file() && file_path.extension().and_then(|s| s.to_str()) == Some("rs") {
                            files.push(file_path);
                        }
                    }
                    Err(e) => eprintln!("Warning: Error reading glob entry: {}", e),
                }
            }
        } else if path.is_file() {
            if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            for entry in WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("rs"))
            {
                files.push(entry.path().to_path_buf());
            }
        }
    }

    // Filter out excluded paths
    if !exclude_patterns.is_empty() {
        files.retain(|file| {
            let file_str = file.to_string_lossy();
            !exclude_patterns.iter().any(|pattern| {
                // Check if the file matches the exclude pattern
                if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
                    // Use glob matching
                    glob::Pattern::new(pattern)
                        .map(|p| p.matches(&file_str))
                        .unwrap_or(false)
                } else {
                    // Simple string matching for non-glob patterns
                    file_str.contains(pattern.as_str())
                }
            })
        });
    }

    Ok(files)
}

fn parse_position(pos: &str) -> Result<InsertPosition> {
    match pos {
        "first" => Ok(InsertPosition::First),
        "last" => Ok(InsertPosition::Last),
        s if s.starts_with("after:") => {
            let name = s.strip_prefix("after:").unwrap().to_string();
            Ok(InsertPosition::After(name))
        }
        s if s.starts_with("before:") => {
            let name = s.strip_prefix("before:").unwrap().to_string();
            Ok(InsertPosition::Before(name))
        }
        _ => anyhow::bail!("Invalid position: {}. Use 'first', 'last', 'after:name', or 'before:name'", pos),
    }
}

/// Print operation-specific hints when no changes were made
fn print_operation_hints(op: &Operation) {
    match op {
        Operation::AddMatchArm(match_op) => {
            if match_op.auto_detect {
                let enum_name = match_op.enum_name.as_deref().unwrap_or("ENUM");
                eprintln!("\n💡 Hints for --auto-detect mode:");
                eprintln!("   • The enum definition must be in the scanned files");
                eprintln!("   • Try: rs-hack find --node-type enum --name {} --paths .", enum_name);
                eprintln!("   • If enum is in another crate, try: --paths . --paths ../other_crate/src");
                eprintln!("   • For external enums, use --match-arm instead (no --auto-detect):");
                eprintln!("     rs-hack add --match-arm \"{}::Variant\" --body \"todo!()\" --paths src", enum_name);
            } else {
                eprintln!("\n💡 Hints for match arm addition:");
                eprintln!("   • Make sure match expressions exist in the scanned files");
                eprintln!("   • Pattern should be like: EnumName::Variant or EnumName::Variant {{ .. }}");
                eprintln!("   • Try: rs-hack find --node-type match-arm --paths src");
            }
        }
        Operation::AddStructField(field_op) => {
            eprintln!("\n💡 Hints:");
            eprintln!("   • Try: rs-hack find --node-type struct --name {} --paths .", field_op.struct_name);
        }
        Operation::AddEnumVariant(variant_op) => {
            eprintln!("\n💡 Hints:");
            eprintln!("   • Try: rs-hack find --node-type enum --name {} --paths .", variant_op.enum_name);
        }
        _ => {
            // Generic hint
            eprintln!("\n💡 Hint: Use rs-hack find to verify targets exist in scanned files");
        }
    }
}

fn execute_operation(
    files: &[PathBuf],
    op: &Operation,
    apply: bool,
    output: Option<&PathBuf>,
    format: &str,
    show_summary: bool,
    limit: Option<usize>,
) -> Result<()> {
    let mut changes = Vec::new();
    let mut total_stats = DiffStats::default();
    let mut total_modifications = 0;
    let mut all_unmatched_paths: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut last_error: Option<anyhow::Error> = None;

    let mut parse_errors: Vec<(PathBuf, String)> = Vec::new();

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;

        let mut editor = match RustEditor::new(&content) {
            Ok(editor) => editor,
            Err(e) => {
                if files.len() == 1 {
                    return Err(e).with_context(|| format!("Failed to parse {}", file_path.display()));
                }
                eprintln!("⚠️  Skipping {}: {}", file_path.display(), e);
                parse_errors.push((file_path.clone(), format!("{}", e)));
                continue;
            }
        };

        match editor.apply_operation(op) {
            Ok(result) => {
                // Collect unmatched qualified paths
                if let Some(unmatched) = result.unmatched_qualified_paths {
                    for (path, count) in unmatched {
                        *all_unmatched_paths.entry(path).or_insert(0) += count;
                    }
                }

                if result.changed {
                    // Track total modifications across all files
                    let modifications_in_file = result.modified_nodes.len();
                    total_modifications += modifications_in_file;

                    let new_content = editor.to_string();
                    changes.push(FileChange {
                        path: file_path.clone(),
                        old_content: content.clone(),
                        new_content: new_content.clone(),
                    });

                    if format == "diff" {
                        // In diff mode, print the diff
                        let stats = print_diff(file_path, &content, &new_content);
                        total_stats.add(&stats);

                        if apply {
                            // Apply changes and show a message
                            let write_path = output.unwrap_or(file_path);
                            std::fs::write(write_path, &new_content)
                                .with_context(|| format!("Failed to write {}", write_path.display()))?;
                        }
                    } else if format == "summary" {
                        // In summary mode, print only changed lines
                        let stats = print_summary_diff(file_path, &content, &new_content);
                        total_stats.add(&stats);

                        if apply {
                            // Apply changes and show a message
                            let write_path = output.unwrap_or(file_path);
                            std::fs::write(write_path, &new_content)
                                .with_context(|| format!("Failed to write {}", write_path.display()))?;
                        }
                    } else {
                        // Default mode - original behavior
                        if apply {
                            let write_path = output.unwrap_or(file_path);
                            std::fs::write(write_path, &new_content)
                                .with_context(|| format!("Failed to write {}", write_path.display()))?;
                            if let Some(out) = output {
                                println!("✓ Written to: {}", out.display());
                            } else {
                                println!("✓ Modified: {}", file_path.display());
                            }
                        } else {
                            if output.is_some() {
                                println!("Would write to: {}", output.unwrap().display());
                            } else {
                                println!("Would modify: {}", file_path.display());
                            }
                        }
                    }

                    // Check if we've hit the limit
                    if let Some(limit) = limit {
                        if total_modifications >= limit {
                            println!("\n⚠️  Limit reached: {} modifications made (limit: {})", total_modifications, limit);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                // Not an error if the target doesn't exist in this file
                if files.len() == 1 {
                    return Err(e);
                }
                // Store for diagnostic output if nothing matches
                last_error = Some(e);
            }
        }
    }

    // Report parse errors summary
    if !parse_errors.is_empty() {
        eprintln!("\n⚠️  {} file(s) skipped due to parse errors:", parse_errors.len());
        for (path, err) in &parse_errors {
            eprintln!("   {} — {}", path.display(), err);
        }
    }

    // Show hint if we found unmatched qualified paths (even if we made some changes)
    if !all_unmatched_paths.is_empty() {
        if !changes.is_empty() {
            println!("\n⚠️  Note: Some instances were not matched:");
        }

        println!("\n💡 Hint: Found {} struct literal(s) with fully qualified paths that didn't match:",
            all_unmatched_paths.values().sum::<usize>());

        let mut paths: Vec<_> = all_unmatched_paths.iter().collect();
        paths.sort_by_key(|(path, _)| *path);

        for (path, count) in &paths {
            println!("   {} ({} instance{})", path, count, if **count == 1 { "" } else { "s" });
        }

        println!("\nTo match all of these, use:");

        // Extract the simple name from the first path for the suggestion
        if let Some((first_path, _)) = paths.first() {
            if let Some(simple_name) = first_path.split("::").last() {
                println!("   rs-hack ... --name \"*::{}\" ...", simple_name);
                println!("\nOr match specific paths:");
                for (path, _) in paths.iter().take(3) {
                    println!("   rs-hack ... --name \"{}\" ...", path);
                }
                if paths.len() > 3 {
                    println!("   (and {} more...)", paths.len() - 3);
                }
            }
        }
    } else if changes.is_empty() {
        println!("No changes made - target not found in any files");
        // Show the last error for context
        if let Some(err) = last_error {
            eprintln!("\n📋 Diagnostic: {}", err);
        }
        // Operation-specific hints
        print_operation_hints(op);
    }

    if format == "diff" && show_summary {
        // Print summary for diff mode
        total_stats.print_summary();
    } else if format == "default" && !apply {
        println!("\n🔍 Dry run complete. Use --apply to make changes, or --format diff to generate a patch.");
        println!("Summary: {} file(s) would be modified", changes.len());
    }

    Ok(())
}

fn execute_batch(batch: &BatchSpec, apply: bool, exclude_patterns: &[String]) -> Result<()> {
    for op in &batch.operations {
        let files = collect_rust_files_with_exclusions(&[batch.base_path.clone()], exclude_patterns)?;
        execute_operation(&files, op, apply, None, "default", false, None)?;
    }
    Ok(())
}

fn execute_operation_with_state(
    files: &[PathBuf],
    op: &Operation,
    apply: bool,
    output: Option<&PathBuf>,
    local_state: &bool,
    format: &str,
    show_summary: bool,
    limit: Option<usize>,
) -> Result<()> {
    // If not applying or output is specified (not in-place), don't track state
    if !apply || output.is_some() {
        return execute_operation(files, op, apply, output, format, show_summary, limit);
    }

    // Generate run ID and get state dir
    let run_id = generate_run_id();
    let state_dir = get_state_dir(*local_state)?;

    // Collect command line for metadata
    let command = std::env::args().collect::<Vec<_>>().join(" ");
    let operation_name = match op {
        Operation::AddStructField(_) => "AddStructField",
        Operation::UpdateStructField(_) => "UpdateStructField",
        Operation::RemoveStructField(_) => "RemoveStructField",
        Operation::AddStructLiteralField(_) => "AddStructLiteralField",
        Operation::AddEnumVariant(_) => "AddEnumVariant",
        Operation::UpdateEnumVariant(_) => "UpdateEnumVariant",
        Operation::RemoveEnumVariant(_) => "RemoveEnumVariant",
        Operation::RenameEnumVariant(_) => "RenameEnumVariant",
        Operation::AddMatchArm(_) => "AddMatchArm",
        Operation::UpdateMatchArm(_) => "UpdateMatchArm",
        Operation::RemoveMatchArm(_) => "RemoveMatchArm",
        Operation::AddImplMethod(_) => "AddImplMethod",
        Operation::AddUseStatement(_) => "AddUseStatement",
        Operation::AddDerive(_) => "AddDerive",
        Operation::Transform(_) => "Transform",
        Operation::RenameFunction(_) => "RenameFunction",
        Operation::AddDocComment(_) => "AddDocComment",
        Operation::UpdateDocComment(_) => "UpdateDocComment",
        Operation::RemoveDocComment(_) => "RemoveDocComment",
        Operation::SetStructLiteralBase(_) => "SetStructLiteralBase",
        Operation::AddCallArg(_) => "AddCallArg",
        Operation::UpdateCallArg(_) => "UpdateCallArg",
        Operation::RemoveCallArg(_) => "RemoveCallArg",
    };

    // First pass: collect changes and backup files
    let mut file_modifications = Vec::new();
    let mut changes_made = false;
    let mut total_stats = DiffStats::default();
    let mut total_modifications = 0;
    let mut all_unmatched_paths: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut last_error: Option<anyhow::Error> = None;

    let mut parse_errors: Vec<(PathBuf, String)> = Vec::new();

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;

        let mut editor = match RustEditor::new(&content) {
            Ok(editor) => editor,
            Err(e) => {
                if files.len() == 1 {
                    return Err(e).with_context(|| format!("Failed to parse {}", file_path.display()));
                }
                eprintln!("⚠️  Skipping {}: {}", file_path.display(), e);
                parse_errors.push((file_path.clone(), format!("{}", e)));
                continue;
            }
        };

        match editor.apply_operation(op) {
            Ok(result) => {
                // Collect unmatched qualified paths
                if let Some(unmatched) = result.unmatched_qualified_paths {
                    for (path, count) in unmatched {
                        *all_unmatched_paths.entry(path).or_insert(0) += count;
                    }
                }

                if result.changed {
                    // Track total modifications across all files
                    let modifications_in_file = result.modified_nodes.len();
                    total_modifications += modifications_in_file;

                    let new_content = editor.to_string();

                    // Print diff if format is "diff" or "summary"
                    if format == "diff" {
                        let stats = print_diff(file_path, &content, &new_content);
                        total_stats.add(&stats);
                    } else if format == "summary" {
                        let stats = print_summary_diff(file_path, &content, &new_content);
                        total_stats.add(&stats);
                    }

                    // Save backup nodes before modifying
                    let hash_before = hash_file(file_path)?;
                    save_backup_nodes(file_path, &result.modified_nodes, &run_id, &state_dir)?;

                    // Write the new content
                    std::fs::write(file_path, &new_content)
                        .with_context(|| format!("Failed to write {}", file_path.display()))?;

                    let hash_after = hash_file(file_path)?;

                    file_modifications.push(FileModification {
                        path: file_path.clone(),
                        hash_before,
                        hash_after,
                        backup_nodes: result.modified_nodes.clone(),
                    });

                    if format != "diff" {
                        println!("✓ Modified: {}", file_path.display());
                    }
                    changes_made = true;

                    // Check if we've hit the limit
                    if let Some(limit) = limit {
                        if total_modifications >= limit {
                            println!("\n⚠️  Limit reached: {} modifications made (limit: {})", total_modifications, limit);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                // Not an error if the target doesn't exist in this file
                if files.len() == 1 {
                    return Err(e);
                }
                // Store for diagnostic output if nothing matches
                last_error = Some(e);
            }
        }
    }

    // Report parse errors summary
    if !parse_errors.is_empty() {
        eprintln!("\n⚠️  {} file(s) skipped due to parse errors:", parse_errors.len());
        for (path, err) in &parse_errors {
            eprintln!("   {} — {}", path.display(), err);
        }
    }

    // Save run metadata if any changes were made
    if !file_modifications.is_empty() {
        let metadata = RunMetadata {
            run_id: run_id.clone(),
            timestamp: chrono::Utc::now(),
            command,
            operation: operation_name.to_string(),
            files_modified: file_modifications,
            status: RunStatus::Applied,
            can_revert: true,
        };

        save_run_metadata(&metadata, &state_dir)?;

        if format == "diff" && show_summary {
            total_stats.print_summary();
        }

        println!("\n📝 Run ID: {} (use 'rs-hack revert {}' to undo)", run_id, run_id);
    } else if !changes_made {
        println!("No changes made - target not found in any files");
        // Show the last error for context
        if let Some(err) = last_error {
            eprintln!("\n📋 Diagnostic: {}", err);
        }
        // Operation-specific hints
        print_operation_hints(op);
    }

    // Show hint if we found unmatched qualified paths (even if we made some changes)
    if !all_unmatched_paths.is_empty() {
        if changes_made {
            println!("\n⚠️  Note: Some instances were not matched:");
        }

        println!("\n💡 Hint: Found {} struct literal(s) with fully qualified paths that didn't match:",
            all_unmatched_paths.values().sum::<usize>());

        let mut paths: Vec<_> = all_unmatched_paths.iter().collect();
        paths.sort_by_key(|(path, _)| *path);

        for (path, count) in &paths {
            println!("   {} ({} instance{})", path, count, if **count == 1 { "" } else { "s" });
        }

        println!("\nTo match all of these, use:");

        // Extract the simple name from the first path for the suggestion
        if let Some((first_path, _)) = paths.first() {
            if let Some(simple_name) = first_path.split("::").last() {
                println!("   rs-hack ... --name \"*::{}\" ...", simple_name);
                println!("\nOr match specific paths:");
                for (path, _) in paths.iter().take(3) {
                    println!("   rs-hack ... --name \"{}\" ...", path);
                }
                if paths.len() > 3 {
                    println!("   (and {} more...)", paths.len() - 3);
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
#[allow(dead_code)]
struct FileChange {
    path: PathBuf,
    old_content: String,
    new_content: String,
}
