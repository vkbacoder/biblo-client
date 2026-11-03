import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { call as invoke } from "../../api";
import type {
  EmployeePolicyOverride,
  ManualScreenshotRequest,
  OrgMember,
  OrgScreenshotSettings,
} from "./rbacTypes";

const svgp = { viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round", strokeLinejoin: "round" } as const;
const IconCamera = () => (<svg {...svgp} width="18" height="18" aria-hidden><path d="M14.5 4h-5L8 6H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-4z" /><circle cx="12" cy="13" r="3.2" /></svg>);
const IconX = () => (<svg {...svgp} width="16" height="16" aria-hidden><line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" /></svg>);
const IconSliders = () => (<svg {...svgp} width="18" height="18" aria-hidden><line x1="4" y1="21" x2="4" y2="14" /><line x1="4" y1="10" x2="4" y2="3" /><line x1="12" y1="21" x2="12" y2="12" /><line x1="12" y1="8" x2="12" y2="3" /><line x1="20" y1="21" x2="20" y2="16" /><line x1="20" y1="12" x2="20" y2="3" /><line x1="1" y1="14" x2="7" y2="14" /><line x1="9" y1="8" x2="15" y2="8" /><line x1="17" y1="16" x2="23" y2="16" /></svg>);

export function ScreenshotManagement({
  businessId,
  canManage = true,
}: {
  businessId: string;
  canManage?: boolean;
}) {
  const { t } = useTranslation();
  const [members, setMembers] = useState<OrgMember[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  // Policy form state
  const [captureEnabled, setCaptureEnabled] = useState(true);
  const [continuousEnabled, setContinuousEnabled] = useState(true);
  const [intervalMinutes, setIntervalMinutes] = useState(10);
  const [manualEnabled, setManualEnabled] = useState(true);
  const [savingPolicy, setSavingPolicy] = useState(false);

  // Manual request state
  const [targetEmployeeId, setTargetEmployeeId] = useState("");
  const [triggeringRequest, setTriggeringRequest] = useState(false);
  const [requests, setRequests] = useState<ManualScreenshotRequest[]>([]);

  // Employee override modal
  const [overrideModalOpen, setOverrideModalOpen] = useState(false);
  const [overrideEmp, setOverrideEmp] = useState<OrgMember | null>(null);
  const [overrideData, setOverrideData] = useState<EmployeePolicyOverride | null>(null);
  const [overrideLoading, setOverrideLoading] = useState(false);

  const loadData = useCallback(async () => {
    if (!businessId) return;
    setLoading(true);
    setError(null);
    try {
      const [sRes, mRes] = await Promise.all([
        invoke<OrgScreenshotSettings>("admin_screenshot_settings", { businessId }).catch(() => null),
        invoke<OrgMember[] | { members: OrgMember[] }>("admin_members", { businessId }).catch(() => []),
      ]);
      if (sRes) {
        setCaptureEnabled(sRes.capture_enabled ?? true);
        setContinuousEnabled(sRes.continuous_capture_enabled ?? true);
        setIntervalMinutes(sRes.capture_interval_minutes ?? 10);
        setManualEnabled(sRes.manual_capture_enabled ?? true);
      }
      const safeMembers = Array.isArray(mRes) ? mRes : (mRes as any)?.members ?? [];
      setMembers(safeMembers);
      if (safeMembers.length > 0 && !targetEmployeeId) {
        setTargetEmployeeId(safeMembers[0].user_id || safeMembers[0].id);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [businessId, targetEmployeeId]);

  useEffect(() => {
    loadData();
  }, [businessId]);

  async function saveOrgSettings(e: React.FormEvent) {
    e.preventDefault();
    if (!canManage || savingPolicy) return;
    setSavingPolicy(true);
    setError(null);
    setSuccess(null);
    try {
      await invoke("admin_update_screenshot_settings", {
        businessId,
        body: {
          capture_enabled: captureEnabled,
          continuous_capture_enabled: continuousEnabled,
          capture_interval_minutes: Number(intervalMinutes),
          manual_capture_enabled: manualEnabled,
        },
      });
      setSuccess(t("screens:screenshot.saved", "Screenshot settings updated successfully."));
      setTimeout(() => setSuccess(null), 3000);
      await loadData();
    } catch (err) {
      setError(String(err));
    } finally {
      setSavingPolicy(false);
    }
  }

  async function triggerManualRequest(e: React.FormEvent) {
    e.preventDefault();
    if (!targetEmployeeId || triggeringRequest) return;
    setTriggeringRequest(true);
    setError(null);
    try {
      const res = await invoke<ManualScreenshotRequest>("admin_create_screenshot_request", {
        businessId,
        employeeId: targetEmployeeId,
      });
      setSuccess("Manual screenshot request sent successfully.");
      setTimeout(() => setSuccess(null), 3000);
      setRequests((prev) => [res, ...prev.slice(0, 19)]);
    } catch (err) {
      setError(String(err));
    } finally {
      setTriggeringRequest(false);
    }
  }

  async function openOverrideModal(member: OrgMember) {
    setOverrideEmp(member);
    setOverrideModalOpen(true);
    setOverrideLoading(true);
    try {
      const pol = await invoke<EmployeePolicyOverride>("admin_employee_screenshot_policy", {
        businessId,
        employeeId: member.user_id || member.id,
      });
      setOverrideData(pol);
    } catch {
      setOverrideData(null);
    } finally {
      setOverrideLoading(false);
    }
  }

  async function saveOverride(patch: Partial<EmployeePolicyOverride>) {
    if (!overrideEmp) return;
    setOverrideLoading(true);
    try {
      await invoke("admin_update_employee_screenshot_policy", {
        businessId,
        employeeId: overrideEmp.user_id || overrideEmp.id,
        body: patch,
      });
      setOverrideModalOpen(false);
      setSuccess(`Policy override saved for ${overrideEmp.display_name}`);
      setTimeout(() => setSuccess(null), 3000);
    } catch (err) {
      setError(String(err));
    } finally {
      setOverrideLoading(false);
    }
  }

  async function resetOverride() {
    if (!overrideEmp) return;
    setOverrideLoading(true);
    try {
      await invoke("admin_reset_employee_screenshot_policy", {
        businessId,
        employeeId: overrideEmp.user_id || overrideEmp.id,
      });
      setOverrideModalOpen(false);
      setSuccess(`Reset policy to org defaults for ${overrideEmp.display_name}`);
      setTimeout(() => setSuccess(null), 3000);
    } catch (err) {
      setError(String(err));
    } finally {
      setOverrideLoading(false);
    }
  }

  return (
    <div className="bb-members">
      <div className="bb-members__head">
        <div>
          <h1 className="bb-adminboard__title">{t("nav.ScreenshotManagement", "Screenshot Policy & Controls")}</h1>
          <p className="bb-adminboard__who">
            Server-enforced screenshot frequencies, employee overrides, and manual capture triggers
          </p>
        </div>
      </div>

      {error && <div className="auth-err" role="alert">{error}</div>}
      {success && <div className="auth-success" style={{ background: "var(--accent-weak)", color: "var(--accent)", padding: "10px 14px", borderRadius: 8, marginBottom: 16 }}>{success}</div>}

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
        {/* Master Policy Card */}
        <div className="card" style={{ padding: 20 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 16 }}>
            <span style={{ color: "var(--accent)" }}><IconSliders /></span>
            <span style={{ fontWeight: 600, fontSize: 16, color: "var(--text)" }}>Master Organization Policy</span>
          </div>

          <form onSubmit={saveOrgSettings}>
            <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
              <label style={{ display: "flex", alignItems: "center", justifyContent: "space-between", cursor: "pointer" }}>
                <div>
                  <div style={{ fontWeight: 600, color: "var(--text)" }}>Enable Screenshot Capture</div>
                  <div style={{ fontSize: 12, color: "var(--text-muted)" }}>Master kill-switch for automated screenshots</div>
                </div>
                <input
                  type="checkbox"
                  checked={captureEnabled}
                  onChange={(e) => setCaptureEnabled(e.target.checked)}
                  disabled={!canManage || savingPolicy}
                />
              </label>

              <label style={{ display: "flex", alignItems: "center", justifyContent: "space-between", cursor: "pointer" }}>
                <div>
                  <div style={{ fontWeight: 600, color: "var(--text)" }}>Continuous Capture</div>
                  <div style={{ fontSize: 12, color: "var(--text-muted)" }}>Periodically capture screens during active working hours</div>
                </div>
                <input
                  type="checkbox"
                  checked={continuousEnabled}
                  onChange={(e) => setContinuousEnabled(e.target.checked)}
                  disabled={!canManage || savingPolicy}
                />
              </label>

              <label style={{ display: "flex", alignItems: "center", justifyContent: "space-between", cursor: "pointer" }}>
                <div>
                  <div style={{ fontWeight: 600, color: "var(--text)" }}>Allow Manager Manual Capture</div>
                  <div style={{ fontSize: 12, color: "var(--text-muted)" }}>Permit authorized managers to trigger real-time captures</div>
                </div>
                <input
                  type="checkbox"
                  checked={manualEnabled}
                  onChange={(e) => setManualEnabled(e.target.checked)}
                  disabled={!canManage || savingPolicy}
                />
              </label>

              <label className="bibo-field" style={{ marginTop: 6 }}>
                <span className="bibo-field__lbl">Default Capture Interval (Minutes)</span>
                <span className="bibo-input">
                  <input
                    type="number"
                    min="1"
                    max="120"
                    value={intervalMinutes}
                    onChange={(e) => setIntervalMinutes(Number(e.target.value))}
                    disabled={!canManage || savingPolicy}
                  />
                </span>
              </label>

              {canManage && (
                <button
                  type="submit"
                  className="bibo-btn bibo-btn--primary"
                  style={{ marginTop: 8 }}
                  disabled={savingPolicy}
                >
                  {savingPolicy ? t("saving", "Saving...") : t("savePolicy", "Update Master Policy")}
                </button>
              )}
            </div>
          </form>
        </div>

        {/* Manual Capture Trigger Card */}
        <div className="card" style={{ padding: 20 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 16 }}>
            <span style={{ color: "var(--accent)" }}><IconCamera /></span>
            <span style={{ fontWeight: 600, fontSize: 16, color: "var(--text)" }}>Trigger Real-Time Capture</span>
          </div>

          <p style={{ fontSize: 13, color: "var(--text-muted)", marginBottom: 16 }}>
            Sends a server-validated manual capture instruction to the active desktop client of the selected employee.
          </p>

          <form onSubmit={triggerManualRequest}>
            <label className="bibo-field">
              <span className="bibo-field__lbl">Target Employee</span>
              <span className="bibo-input">
                <select
                  value={targetEmployeeId}
                  onChange={(e) => setTargetEmployeeId(e.target.value)}
                  disabled={triggeringRequest}
                >
                  {members.map((m) => (
                    <option key={m.user_id || m.id} value={m.user_id || m.id}>
                      {m.display_name} ({m.email || m.username})
                    </option>
                  ))}
                </select>
              </span>
            </label>

            <button
              type="submit"
              className="bibo-btn bibo-btn--primary"
              style={{ marginTop: 12, width: "100%" }}
              disabled={triggeringRequest || !targetEmployeeId}
            >
              <IconCamera /> {triggeringRequest ? "Requesting capture..." : "Trigger Screenshot Now"}
            </button>
          </form>

          {requests.length > 0 && (
            <div style={{ marginTop: 20 }}>
              <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 8 }}>Recent Capture Requests</div>
              <div style={{ display: "flex", flexDirection: "column", gap: 6, maxHeight: 150, overflowY: "auto" }}>
                {requests.map((r) => (
                  <div
                    key={r.id}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      padding: "6px 10px",
                      background: "var(--bg-subtle)",
                      borderRadius: 6,
                      fontSize: 12,
                    }}
                  >
                    <span>ID: {r.id.slice(0, 8)}...</span>
                    <span className="bibo-badge" style={{ textTransform: "capitalize" }}>
                      {r.status}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Per-Employee Overrides Table */}
      <div className="card bb-adminboard__tablecard" style={{ marginTop: 16 }}>
        <div style={{ padding: "16px 20px", display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <div>
            <span style={{ fontWeight: 600, color: "var(--text)" }}>Employee Screenshot Policies</span>
            <div style={{ fontSize: 12, color: "var(--text-muted)" }}>
              Customize capture intervals or exemptions for specific individuals
            </div>
          </div>
        </div>

        {loading && members.length === 0 ? (
          <div className="muted bb-adminboard__msg">{t("loading")}</div>
        ) : (
          <table className="bb-adminboard__table">
            <thead>
              <tr>
                <th>{t("screens:admin.employees", "Employee")}</th>
                <th>{t("screens:admin.login", "Login")}</th>
                <th>Role</th>
                <th>Policy Override</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {members.map((m) => (
              <tr key={m.user_id || m.id}>
                <td>
                  <div style={{ fontWeight: 600, color: "var(--text)" }}>{m.display_name}</div>
                </td>
                <td className="bb-adminboard__login">{m.email || m.username}</td>
                <td>
                  <span className="bibo-badge">{m.role}</span>
                </td>
                <td>
                  <span style={{ fontSize: 12, color: "var(--text-muted)" }}>
                    Standard Org Default ({intervalMinutes}m)
                  </span>
                </td>
                <td className="r">
                  {canManage && (
                    <button
                      type="button"
                      className="bibo-btn bibo-btn--secondary"
                      style={{ padding: "4px 8px", fontSize: 12 }}
                      onClick={() => openOverrideModal(m)}
                    >
                      Configure Override
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        )}
      </div>

      {/* Override Modal */}
      {overrideModalOpen && overrideEmp && (
        <div className="bb-modal-overlay" onClick={() => !overrideLoading && setOverrideModalOpen(false)}>
          <div className="bibo-dlg" onClick={(e) => e.stopPropagation()}>
            <div className="bibo-dlg__head">
              <div className="bibo-dlg__icon"><IconSliders /></div>
              <div className="bibo-dlg__title">Policy Override: {overrideEmp.display_name}</div>
              <button type="button" className="bibo-dlg__close" onClick={() => setOverrideModalOpen(false)}>
                <IconX />
              </button>
            </div>
            <div className="bibo-dlg__body">
              {overrideLoading ? (
                <div className="muted">{t("loading")}</div>
              ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
                  <label className="bibo-field">
                    <span className="bibo-field__lbl">Custom Interval (Minutes)</span>
                    <span className="bibo-input">
                      <input
                        type="number"
                        min="1"
                        max="120"
                        defaultValue={overrideData?.capture_interval_minutes ?? intervalMinutes}
                        id="override_interval"
                      />
                    </span>
                  </label>

                  <label style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                    <span>Enable Capture for this Employee</span>
                    <input
                      type="checkbox"
                      defaultChecked={overrideData?.capture_enabled ?? true}
                      id="override_enabled"
                    />
                  </label>
                </div>
              )}
            </div>
            <div className="bibo-dlg__foot">
              <button
                type="button"
                className="bibo-btn bibo-btn--ghost"
                onClick={resetOverride}
                disabled={overrideLoading}
                title="Reset to organization default policy"
              >
                Reset to Org Default
              </button>
              <button
                type="button"
                className="bibo-btn bibo-btn--primary"
                onClick={() => {
                  const intEl = document.getElementById("override_interval") as HTMLInputElement;
                  const enEl = document.getElementById("override_enabled") as HTMLInputElement;
                  saveOverride({
                    capture_interval_minutes: Number(intEl?.value || intervalMinutes),
                    capture_enabled: enEl?.checked ?? true,
                  });
                }}
                disabled={overrideLoading}
              >
                Save Override
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
