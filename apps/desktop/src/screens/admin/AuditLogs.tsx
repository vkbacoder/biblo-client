import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { call as invoke } from "../../api";
import type { OrgAuditLog } from "./rbacTypes";

const svgp = { viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round", strokeLinejoin: "round" } as const;
const IconRefresh = () => (<svg {...svgp} width="14" height="14" aria-hidden><polyline points="23 4 23 10 17 10" /><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" /></svg>);

export function AuditLogs({ businessId }: { businessId: string }) {
  const { t } = useTranslation();
  const [logs, setLogs] = useState<OrgAuditLog[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadLogs = useCallback(async () => {
    if (!businessId) return;
    setLoading(true);
    setError(null);
    try {
      const res = await invoke<OrgAuditLog[] | { audit_logs: OrgAuditLog[] }>("admin_audit_logs", {
        businessId,
      });
      const list = Array.isArray(res) ? res : (res as any)?.audit_logs ?? [];
      setLogs(list);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [businessId]);

  useEffect(() => {
    loadLogs();
  }, [businessId]);

  const list = logs ?? [];

  return (
    <div className="bb-members">
      <div className="bb-members__head">
        <div>
          <h1 className="bb-adminboard__title">Organizational Audit Logs</h1>
          <p className="bb-adminboard__who">
            Immutable security and administrative actions trail for enterprise compliance
          </p>
        </div>
        <div className="bb-members__actions">
          <button
            type="button"
            className="bibo-btn bibo-btn--secondary"
            onClick={loadLogs}
            disabled={loading}
          >
            <IconRefresh /> Refresh
          </button>
        </div>
      </div>

      {error && <div className="auth-err" role="alert">{error}</div>}

      <div className="card bb-adminboard__tablecard">
        {loading && logs === null ? (
          <div className="muted bb-adminboard__msg">{t("loading")}</div>
        ) : list.length === 0 ? (
          <div className="muted bb-adminboard__msg">No audit log records found.</div>
        ) : (
          <table className="bb-adminboard__table">
            <thead>
              <tr>
                <th>Timestamp</th>
                <th>Actor</th>
                <th>Action</th>
                <th>Target</th>
                <th>Details</th>
              </tr>
            </thead>
            <tbody>
              {list.map((log) => (
                <tr key={log.id}>
                  <td style={{ fontSize: 12, color: "var(--text-muted)", whiteSpace: "nowrap" }}>
                    {new Date(log.created_at).toLocaleString()}
                  </td>
                  <td>
                    <div style={{ fontWeight: 600, color: "var(--text)" }}>{log.actor_name || log.actor_user_id}</div>
                  </td>
                  <td>
                    <span
                      className="bibo-badge"
                      style={{
                        background: log.action.includes("delete") || log.action.includes("suspend")
                          ? "var(--data-rose)"
                          : log.action.includes("promote")
                            ? "var(--accent)"
                            : "var(--bg-subtle)",
                        color: log.action.includes("delete") || log.action.includes("suspend") || log.action.includes("promote")
                          ? "#fff"
                          : "var(--text)",
                      }}
                    >
                      {log.action.replace(/_/g, " ")}
                    </span>
                  </td>
                  <td>
                    <div style={{ fontSize: 13, color: "var(--text)" }}>
                      {log.target_name || log.target_id || log.target_type}
                    </div>
                    <div style={{ fontSize: 11, color: "var(--text-muted)" }}>
                      Type: {log.target_type}
                    </div>
                  </td>
                  <td style={{ fontSize: 12, color: "var(--text-muted)", maxWidth: 300 }}>
                    {log.metadata ? JSON.stringify(log.metadata) : "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
