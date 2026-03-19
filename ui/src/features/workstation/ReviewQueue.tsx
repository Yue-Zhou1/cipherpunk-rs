import { useEffect, useMemo, useRef, useState } from "react";

import {
  applyReviewDecision,
  loadReviewQueue,
  type ReviewDecisionAction,
  type ReviewQueueItem,
} from "../../ipc/commands";

type ReviewQueueProps = {
  sessionId: string;
  selectedRecordId?: string | null;
  onSelectRecord?: (item: ReviewQueueItem) => void;
};

function ReviewQueue({
  sessionId,
  selectedRecordId = null,
  onSelectRecord,
}: ReviewQueueProps): JSX.Element {
  const [items, setItems] = useState<ReviewQueueItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busyRecordId, setBusyRecordId] = useState<string | null>(null);
  const onSelectRecordRef = useRef(onSelectRecord);

  useEffect(() => {
    onSelectRecordRef.current = onSelectRecord;
  }, [onSelectRecord]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);

    void loadReviewQueue(sessionId)
      .then((response) => {
        if (!cancelled) {
          setItems(response.items);
          if (!selectedRecordId && response.items.length > 0) {
            onSelectRecordRef.current?.(response.items[0]);
          }
        }
      })
      .catch(() => {
        if (!cancelled) {
          setItems([]);
          setError("Unable to load review queue.");
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [selectedRecordId, sessionId]);

  const pendingCount = useMemo(
    () => items.filter((item) => item.verificationStatus === "unverified").length,
    [items]
  );

  const applyDecision = (recordId: string, action: ReviewDecisionAction): void => {
    const note = action === "annotate" ? window.prompt("Add note", "")?.trim() : undefined;

    setBusyRecordId(recordId);
    setError(null);

    void applyReviewDecision(sessionId, {
      recordId,
      action,
      note: note && note.length > 0 ? note : undefined,
    })
      .then((response) => {
        setItems((current) =>
          current.map((item) => (item.recordId === response.item.recordId ? response.item : item))
        );
        if (response.item.recordId === selectedRecordId) {
          onSelectRecordRef.current?.(response.item);
        }
      })
      .catch(() => {
        setError("Unable to apply review decision.");
      })
      .finally(() => {
        setBusyRecordId(null);
      });
  };

  return (
    <section className="panel workstation-review-queue" aria-label="Review Queue">
      <div className="workstation-panel-head">
        <p className="workstation-panel-eyebrow">Review</p>
        <h2>Review Queue</h2>
      </div>
      <p className="muted-text">
        {pendingCount} pending candidate{pendingCount === 1 ? "" : "s"}
      </p>

      {loading ? <p className="muted-text">Loading review queue...</p> : null}
      {error ? <p className="banner banner-error">{error}</p> : null}

      {!loading && !error && items.length === 0 ? (
        <p className="muted-text">No candidates pending review.</p>
      ) : null}

      {!loading && items.length > 0 ? (
        <ul className="toolbench-list" aria-label="Review candidates">
          {items.map((item) => {
            const busy = busyRecordId === item.recordId;
            const active = item.recordId === selectedRecordId;
            return (
              <li
                key={item.recordId}
                className={`review-queue-item${active ? " selected" : ""}`}
              >
                <div className="review-queue-item-head">
                  <button
                    type="button"
                    className="review-queue-select inline-action"
                    onClick={() => onSelectRecordRef.current?.(item)}
                  >
                    {item.title}
                  </button>
                  <span className="muted-text">{item.verificationStatus}</span>
                </div>
                <p className="muted-text">{item.summary}</p>
                <div className="review-queue-actions">
                  <button
                    type="button"
                    className="inline-action"
                    disabled={busy}
                    onClick={() => applyDecision(item.recordId, "confirm")}
                  >
                    Confirm Finding
                  </button>
                  <button
                    type="button"
                    className="inline-action"
                    disabled={busy}
                    onClick={() => applyDecision(item.recordId, "reject")}
                  >
                    Mark False Positive
                  </button>
                  <button
                    type="button"
                    className="inline-action"
                    disabled={busy}
                    onClick={() => applyDecision(item.recordId, "suppress")}
                  >
                    Suppress
                  </button>
                  <button
                    type="button"
                    className="inline-action"
                    disabled={busy}
                    onClick={() => applyDecision(item.recordId, "annotate")}
                  >
                    Annotate
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      ) : null}
    </section>
  );
}

export default ReviewQueue;
