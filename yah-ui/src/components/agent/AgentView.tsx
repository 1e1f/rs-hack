import { SessionList } from "./SessionList";
import { SessionPane } from "./SessionPane";
import { Splash } from "../shared/Splash";
import { mockSession, mockSessionList } from "../../mock";

interface AgentViewProps {
  relayId: string | null;
  onSelectRelay?: (relayId: string) => void;
}

export function AgentView({ relayId, onSelectRelay }: AgentViewProps) {
  const activeRow = mockSessionList.find((s) => s.relayId === relayId);

  return (
    <div className="flex h-full min-h-0">
      <SessionList
        sessions={mockSessionList}
        activeRelayId={relayId}
        onSelect={onSelectRelay}
      />
      {relayId && activeRow ? (
        <SessionPane session={mockSession} title={activeRow.title} />
      ) : (
        <div className="flex flex-1 items-center justify-center bg-paper/90">
          <Splash
            variant="lantern"
            caption={
              relayId ? `No agent has visited ${relayId} yet` : "No relay selected"
            }
            sub={
              relayId
                ? `Start a session on ${relayId} to begin the conversation. The agent picks up the relay's @yah:next steps and works from there.`
                : "Pick a relay from the rail or the title-bar selector to view its agent session."
            }
          />
        </div>
      )}
    </div>
  );
}
