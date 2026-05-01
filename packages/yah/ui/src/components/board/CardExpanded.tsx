import type { ReactNode } from "react";
import { Icon } from "../shared/Glyph";
import type { Ticket } from "../../types";

/* Expanded body of a TicketCard. Renders Handoff / Next steps / Gotchas /
   Verify / References in printed-page section style — small-caps eyebrows
   + body content in mixed serif (handoff prose) / sans (lists) / mono
   (verify shell). References are `@arch:see` links — rendered as clickable
   chips that route through the yah:// scheme: a doc under
   `.yah/arch/authored/` opens in the arch tab via `yah://arch/doc/<rel>`,
   anything else falls back to `yah://file/<rel>` which jumps to the
   files/editor surface.
*/
export function CardExpanded({
  ticket: t,
  onYahLink,
}: {
  ticket: Ticket;
  onYahLink?: (href: string) => void;
}) {
  const hasHandoff = (t.handoff?.length ?? 0) > 0;
  const hasNext = (t.nextSteps?.length ?? 0) > 0;
  const hasGotchas = (t.gotchas?.length ?? 0) > 0;
  const hasVerify = (t.verify?.length ?? 0) > 0;
  const hasSeeAlso = (t.seeAlso?.length ?? 0) > 0;
  if (!hasHandoff && !hasNext && !hasGotchas && !hasVerify && !hasSeeAlso)
    return null;

  return (
    <div className="mt-2 flex flex-col gap-2.5 border-t border-rule/50 pt-2">
      {hasHandoff && (
        <Section label="Handoff">
          {t.handoff!.map((h, i) => (
            <p
              key={i}
              className="m-0 font-display text-[13px] leading-[1.5] text-ink-2"
            >
              {h}
            </p>
          ))}
        </Section>
      )}
      {hasNext && (
        <Section label="Next steps">
          <ul className="m-0 list-none p-0">
            {t.nextSteps!.map((s, i) => (
              <li
                key={i}
                className="mb-[3px] flex gap-1.5 text-[12px] text-ink-2"
              >
                <span className="font-display leading-[1.2] text-accent">
                  ·
                </span>
                <span className="flex-1">{s}</span>
              </li>
            ))}
          </ul>
        </Section>
      )}
      {hasGotchas && (
        <Section label="Gotchas" tone="bug">
          <ul className="m-0 list-none p-0">
            {t.gotchas!.map((g, i) => (
              <li
                key={i}
                className="mb-[3px] flex gap-1.5 text-[12px] text-ink-2"
              >
                <span className="mt-[2px] shrink-0 text-st-bug">
                  <Icon name="warning" size={12} />
                </span>
                <span>{g}</span>
              </li>
            ))}
          </ul>
        </Section>
      )}
      {hasVerify && (
        <Section label="Verify">
          <ul className="m-0 list-none p-0">
            {t.verify!.map((v, i) => (
              <li
                key={i}
                className="mb-[3px] rounded-[3px] bg-[color-mix(in_oklab,var(--color-paper-3)_35%,transparent)] px-1.5 py-[3px] font-mono text-[11px] text-ink-2"
              >
                {v}
              </li>
            ))}
          </ul>
        </Section>
      )}
      {hasSeeAlso && (
        <Section label="References">
          <div className="flex flex-wrap gap-1">
            {t.seeAlso!.map((path, i) => (
              <SeeAlsoChip key={i} path={path} onYahLink={onYahLink} />
            ))}
          </div>
        </Section>
      )}
    </div>
  );
}

const ARCH_AUTHORED_PREFIX = ".yah/arch/authored/";

function isAuthoredDoc(path: string): boolean {
  if (!path.startsWith(ARCH_AUTHORED_PREFIX)) return false;
  const lower = path.toLowerCase();
  return lower.endsWith(".md") || lower.endsWith(".mmd");
}

function chipLabel(path: string): string {
  /* Authored docs typically live under `.yah/arch/authored/<topic>.md`;
     the topic is the only part the user authored, so that's what shows.
     Keeps the chip readable without losing the path on hover (title=). */
  if (path.startsWith(ARCH_AUTHORED_PREFIX)) {
    const rest = path.slice(ARCH_AUTHORED_PREFIX.length);
    return rest.replace(/\.(md|mmd)$/i, "");
  }
  const slash = path.lastIndexOf("/");
  return slash >= 0 ? path.slice(slash + 1) : path;
}

function SeeAlsoChip({
  path,
  onYahLink,
}: {
  path: string;
  onYahLink?: (href: string) => void;
}) {
  const href = isAuthoredDoc(path)
    ? `yah://arch/doc/${path}`
    : `yah://file/${path}`;
  const label = chipLabel(path);
  const stop = (e: React.MouseEvent | React.PointerEvent) => e.stopPropagation();
  if (!onYahLink) {
    /* No router wired (e.g. some test harness) — render as a static chip
       so the data still surfaces; click is a no-op. */
    return (
      <span
        title={path}
        className="rounded border border-rule/40 bg-paper-3/30 px-1.5 py-[2px] font-mono text-[11px] text-ink-3"
      >
        {label}
      </span>
    );
  }
  return (
    <button
      title={path}
      onClick={(e) => {
        stop(e);
        onYahLink(href);
      }}
      onPointerDown={stop}
      className="rounded border border-rule/40 bg-paper-3/30 px-1.5 py-[2px] font-mono text-[11px] text-accent hover:bg-paper-3/50 hover:text-accent-2"
    >
      {label}
    </button>
  );
}

function Section({
  label,
  tone,
  children,
}: {
  label: string;
  tone?: "bug";
  children: ReactNode;
}) {
  return (
    <div>
      <div
        className={`eyebrow mb-1 ${tone === "bug" ? "text-st-bug" : "text-ink-3"}`}
      >
        {label}
      </div>
      {children}
    </div>
  );
}
