import type { ReactNode } from "react";
import { Icon } from "../shared/Glyph";
import type { Ticket } from "../../types";

/* Expanded body of a TicketCard. Renders Handoff / Next steps / Gotchas /
   Verify in printed-page section style — small-caps eyebrows + body content
   in mixed serif (handoff prose) / sans (lists) / mono (verify shell). */
export function CardExpanded({ ticket: t }: { ticket: Ticket }) {
  const hasHandoff = (t.handoff?.length ?? 0) > 0;
  const hasNext = (t.nextSteps?.length ?? 0) > 0;
  const hasGotchas = (t.gotchas?.length ?? 0) > 0;
  const hasVerify = (t.verify?.length ?? 0) > 0;
  if (!hasHandoff && !hasNext && !hasGotchas && !hasVerify) return null;

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
    </div>
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
