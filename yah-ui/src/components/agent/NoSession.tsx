import { Icon } from "../shared/Glyph";
import { Splash } from "../shared/Splash";

interface NoSessionProps {
  relayId: string | null;
  onStart?: (relayId: string) => void;
}

export function NoSession({ relayId, onStart }: NoSessionProps) {
  return (
    <div className="flex flex-1 items-center justify-center bg-paper/90">
      <div className="flex flex-col items-center">
        <Splash
          variant="lantern"
          caption={relayId ? "No session yet" : "No relay selected"}
          sub={
            relayId
              ? "Start an agent on this relay to begin. The session runs server-side on the rig host."
              : "Pick a relay from the rail or the title-bar selector to view or start its agent session."
          }
        />
        {relayId && (
          <button
            onClick={() => onStart?.(relayId)}
            className="mt-2 flex items-center gap-1.5 rounded border border-accent/40 bg-accent px-3 py-1.5 text-[12px] text-vellum hover:bg-accent-2"
          >
            <Icon name="play" size={12} />
            start agent on {relayId}
          </button>
        )}
      </div>
    </div>
  );
}
