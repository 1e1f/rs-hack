import { SessionList } from "./SessionList";
import { SessionPane } from "./SessionPane";
import { mockSession, mockSessionList } from "../../mock";

interface AgentViewProps {
  relayId: string | null;
}

export function AgentView({ relayId }: AgentViewProps) {
  const activeRow = mockSessionList.find((s) => s.relayId === relayId);

  return (
    <div className="flex h-full">
      <SessionList sessions={mockSessionList} activeRelayId={relayId} />
      {relayId && activeRow ? (
        <SessionPane session={mockSession} />
      ) : (
        <div className="flex flex-1 items-center justify-center">
          <div className="max-w-sm text-center">
            <div className="mb-3 text-text-muted">no session</div>
            <p className="text-[12px] leading-relaxed text-text-dim">
              {relayId
                ? `No active session for ${relayId}. Start one to begin.`
                : "Select a relay to view or start its agent session."}
            </p>
            {relayId && (
              <button className="mt-4 rounded border border-border bg-elevated px-3 py-1.5 text-[12px] text-text hover:border-blue/40 hover:bg-blue/10">
                Start agent on {relayId}
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
