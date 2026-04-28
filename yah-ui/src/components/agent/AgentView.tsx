import { NoSession } from "./NoSession";
import { SessionList } from "./SessionList";
import { SessionPane } from "./SessionPane";
import { mockSession, mockSessionList } from "../../mock";

interface AgentViewProps {
  relayId: string | null;
  onSelectRelay?: (relayId: string) => void;
  onJumpToFile?: (fileColon: string) => void;
}

export function AgentView({
  relayId,
  onSelectRelay,
  onJumpToFile,
}: AgentViewProps) {
  const activeRow = mockSessionList.find((s) => s.relayId === relayId);

  return (
    <div className="flex h-full min-h-0">
      <SessionList
        sessions={mockSessionList}
        activeRelayId={relayId}
        onSelect={onSelectRelay}
      />
      {relayId && activeRow ? (
        <SessionPane
          session={mockSession}
          title={activeRow.title}
          onJumpToFile={onJumpToFile}
        />
      ) : (
        <NoSession relayId={relayId} />
      )}
    </div>
  );
}
