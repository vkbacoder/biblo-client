import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { call as invoke } from "../../api";
import type { RoleAccessState } from "../../hooks/useRoleAccess";
import { AVATAR_PALETTE, initials, localDayStart, type OwnerBusiness, type RosterEntry } from "./AdminDashboard";
import { EmployeeDetail } from "./EmployeeDetail";
import type { RbacPermission } from "./rbacTypes";

const svgp = { viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round", strokeLinejoin: "round" } as const;
const IconPlus = () => (<svg {...svgp} width="16" height="16" aria-hidden><path d="M5 12h14" /><path d="M12 5v14" /></svg>);
const IconDice = () => (<svg {...svgp} width="15" height="15" aria-hidden><rect x="3" y="3" width="18" height="18" rx="4" /><path d="M8 8h.01M16 8h.01M8 16h.01M16 16h.01M12 12h.01" /></svg>);
const IconCopy = () => (<svg {...svgp} width="14" height="14" aria-hidden><rect x="9" y="9" width="13" height="13" rx="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></svg>);
const IconArrow = () => (<svg {...svgp} width="15" height="15" aria-hidden><path d="M5 12h14" /><path d="m12 5 7 7-7 7" /></svg>);
const IconX = () => (<svg {...svgp} width="18" height="18" aria-hidden><path d="M18 6 6 18M6 6l12 12" /></svg>);
const IconHome = () => (<svg {...svgp} width="20" height="20" aria-hidden><path d="M3 10.5 12 3l9 7.5" /><path d="M5 9.5V21h14V9.5" /><path d="M9 21v-6h6v6" /></svg>);
const IconTeam = () => (<svg {...svgp} width="20" height="20" aria-hidden><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M22 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" /></svg>);
const IconUserPlus = () => (<svg {...svgp} width="20" height="20" aria-hidden><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M19 8v6M22 11h-6" /></svg>);
const IconCheck = () => (<svg {...svgp} width="20" height="20" aria-hidden><path d="M20 6 9 17l-5-5" /></svg>);
const IconShield = () => (<svg {...svgp} width="15" height="15" aria-hidden><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" /></svg>);

const ALL_PERMISSIONS: { key: RbacPermission; label: string; desc: string }[] = [
  { key: "can_view_team", label: "View Team", desc: "View assigned team members and activities" },
  { key: "can_manage_employees", label: "Manage Employees", desc: "Create, suspend, and configure employees" },
  { key: "can_create_team_leads", label: "Manage Team Leads", desc: "Promote members to team leads and assign teams" },
  { key: "can_assign_employees", label: "Assign Employees", desc: "Assign employees to teams" },
  { key: "can_manage_screenshot_policy", label: "Manage Screenshot Policy", desc: "Configure capture frequencies and triggers" },
  { key: "can_view_screenshots", label: "View Screenshots", desc: "Inspect employee desktop screenshots" },
  { key: "can_view_reports", label: "View Reports", desc: "Access productivity, application, and attendance reports" },
  { key: "can_manage_team", label: "Manage Teams", desc: "Create, rename, and delete teams" },
  { key: "can_view_audit_logs", label: "View Audit Logs", desc: "Access organizational audit timeline" },
];

type Status = "active" | "idle" | "offline";
function memberStatus(lastSeen: number | null): Status {
  if (!lastSeen) return "offline";
  const age = Date.now() / 1000 - lastSeen;
  if (age < 5 * 60) return "active";
  if (age < 30 * 60) return "idle";
  return "offline";
}

function genPassword(): string {
  const chars = "abcdefghijkmnpqrstuvwxyz23456789";
  let out = "";
  for (let i = 0; i < 10; i++) out += chars[Math.floor(Math.random() * chars.length)];
  return out;
}

type Created = { login: string; password: string };

export function Members({
  businessId,
  businessName,
  businessKind,
  onWorkspaceCreated,
  detail,
  onView,
  roleAccess,
  viewerEmployeeId,
}: {
  businessId: string;
  businessName: string;
  businessKind: string;
  onWorkspaceCreated: (b: OwnerBusiness) => void;
  detail: RosterEntry | null;
  onView: (e: RosterEntry | null) => void;
  roleAccess?: RoleAccessState;
  viewerEmployeeId?: string | null;
}) {
  const { t } = useTranslation();
  const [rows, setRows] = useState<RosterEntry[] | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const [search, setSearch] = useState("");

  // Create Workspace Modal
  const [wsOpen, setWsOpen] = useState(false);
  const [wsName, setWsName] = useState("");
  const [wsBusy, setWsBusy] = useState(false);
  const [wsError, setWsError] = useState<string | null>(null);
  const isFamily = businessKind === "family";
  const newWorkspaceLabel = isFamily ? t("screens:admin.newFamily") : t("screens:admin.newTeam");
  const wsNameLabel = isFamily ? t("screens:admin.workspaceNameFamily") : t("screens:admin.workspaceNameTeam");
  const wsPlaceholder = isFamily ? t("screens:admin.workspacePhFamily") : t("screens:admin.workspacePhTeam");

  // Create Member Modal
  const [open, setOpen] = useState(false);
  const [displayName, setDisplayName] = useState("");
  const [login, setLogin] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [created, setCreated] = useState<Created | null>(null);
  const [copied, setCopied] = useState(false);

  // Granular Permissions Modal
  const [permModalMember, setPermModalMember] = useState<RosterEntry | null>(null);
  const [memberPerms, setMemberPerms] = useState<string[]>([]);
  const [permBusy, setPermBusy] = useState(false);

  // Status message
  const [actionMsg, setActionMsg] = useState<string | null>(null);

  const load = useCallback(() => {
    setRows(null);
    setListError(null);
    invoke<RosterEntry[]>("admin_roster", { businessId, dayStart: localDayStart() })
      .then((data) => {
        setRows(data);
      })
      .catch((e) => setListError(String(e)));
  }, [businessId]);

  useEffect(() => {
    load();
  }, [load]);

  const canCreateMembers = roleAccess?.isAdmin || roleAccess?.can("can_manage_employees");
  const canManageRoles = roleAccess?.isAdmin || roleAccess?.can("can_create_team_leads");
  const canManagePerms = roleAccess?.isAdmin;

  async function submitWorkspace(e: React.FormEvent) {
    e.preventDefault();
    if (wsBusy) return;
    const name = wsName.trim();
    if (!name) {
      setWsError(t("screens:admin.workspaceNameError"));
      return;
    }
    setWsBusy(true);
    setWsError(null);
    try {
      const biz = await invoke<OwnerBusiness>("admin_create_business", { name });
      setWsOpen(false);
      setWsName("");
      onWorkspaceCreated(biz);
    } catch (err) {
      setWsError(String(err));
    } finally {
      setWsBusy(false);
    }
  }

  function openModal() {
    setDisplayName("");
    setLogin("");
    setPassword(genPassword());
    setFormError(null);
    setCreated(null);
    setCopied(false);
    setOpen(true);
  }

  function closeModal() {
    setOpen(false);
    if (created) load();
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (busy) return;
    const name = displayName.trim();
    const id = login.trim();
    if (!name || !id || password.length < 8) {
      setFormError(t("screens:admin.memberFormError"));
      return;
    }
    setBusy(true);
    setFormError(null);
    const isEmail = id.includes("@");
    try {
      await invoke("admin_create_employee", {
        businessId,
        displayName: name,
        email: isEmail ? id : null,
        username: isEmail ? null : id,
        password,
      });
      setCreated({ login: id, password });
    } catch (err) {
      setFormError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function copyCreds() {
    if (!created) return;
    try {
      await navigator.clipboard.writeText(`${created.login} / ${created.password}`);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      /* clipboard unavailable */
    }
  }

  // Member role actions
  async function handlePromote(emp: RosterEntry) {
    if (!window.confirm(`Promote ${emp.display_name} to Team Lead?`)) return;
    try {
      await invoke("admin_promote_member", {
        businessId,
        userId: emp.id,
        body: {},
      });
      setActionMsg(`Promoted ${emp.display_name} to Team Lead.`);
      setTimeout(() => setActionMsg(null), 3000);
      load();
    } catch (err) {
      alert(`Promotion failed: ${err}`);
    }
  }

  async function handleDemote(emp: RosterEntry) {
    if (!window.confirm(`Demote ${emp.display_name} from Team Lead to Employee?`)) return;
    try {
      await invoke("admin_demote_member", {
        businessId,
        userId: emp.id,
        body: {},
      });
      setActionMsg(`Demoted ${emp.display_name} to Employee.`);
      setTimeout(() => setActionMsg(null), 3000);
      load();
    } catch (err) {
      alert(`Demotion failed: ${err}`);
    }
  }

  async function handleSuspend(emp: RosterEntry) {
    if (!window.confirm(`Suspend access for ${emp.display_name}?`)) return;
    try {
      await invoke("admin_suspend_member", {
        businessId,
        userId: emp.id,
        body: { reason: "Admin suspension" },
      });
      setActionMsg(`Suspended ${emp.display_name}.`);
      setTimeout(() => setActionMsg(null), 3000);
      load();
    } catch (err) {
      alert(`Suspend failed: ${err}`);
    }
  }

  async function handleReactivate(emp: RosterEntry) {
    try {
      await invoke("admin_reactivate_member", {
        businessId,
        userId: emp.id,
      });
      setActionMsg(`Reactivated ${emp.display_name}.`);
      setTimeout(() => setActionMsg(null), 3000);
      load();
    } catch (err) {
      alert(`Reactivation failed: ${err}`);
    }
  }

  async function handleRemove(emp: RosterEntry) {
    if (!window.confirm(`Permanently remove ${emp.display_name} from the organization?`)) return;
    try {
      await invoke("admin_remove_member", {
        businessId,
        userId: emp.id,
      });
      setActionMsg(`Removed ${emp.display_name}.`);
      setTimeout(() => setActionMsg(null), 3000);
      load();
    } catch (err) {
      alert(`Removal failed: ${err}`);
    }
  }

  async function openPermsModal(emp: RosterEntry) {
    setPermModalMember(emp);
    setPermBusy(true);
    try {
      const res = await invoke<{ permissions: string[] } | string[]>("admin_member_permissions", {
        businessId,
        userId: emp.id,
      });
      const list = Array.isArray(res) ? res : (res as any)?.permissions ?? [];
      setMemberPerms(list);
    } catch {
      setMemberPerms([]);
    } finally {
      setPermBusy(false);
    }
  }

  async function togglePermission(permission: string, grant: boolean) {
    if (!permModalMember || permBusy) return;
    setPermBusy(true);
    try {
      if (grant) {
        await invoke("admin_grant_permission", {
          businessId,
          userId: permModalMember.id,
          body: { permission },
        });
        setMemberPerms((prev) => [...prev, permission]);
      } else {
        await invoke("admin_revoke_permission", {
          businessId,
          userId: permModalMember.id,
          permission,
        });
        setMemberPerms((prev) => prev.filter((p) => p !== permission));
      }
    } catch (err) {
      alert(`Permission update failed: ${err}`);
    } finally {
      setPermBusy(false);
    }
  }

  if (detail) {
    const isTeamLeadSelf = Boolean(
      roleAccess?.isTeamLead &&
      viewerEmployeeId &&
      (detail.id === viewerEmployeeId)
    );
    return (
      <EmployeeDetail
        employee={detail}
        onBack={() => onView(null)}
        isTeamLeadSelf={isTeamLeadSelf}
        canManagePolicy={roleAccess?.isAdmin || roleAccess?.can("can_manage_screenshot_policy")}
        businessId={businessId}
      />
    );
  }

  const rawList = rows ?? [];
  const list = rawList.filter((e) => {
    if (!search.trim()) return true;
    const q = search.toLowerCase();
    return (
      e.display_name.toLowerCase().includes(q) ||
      (e.email && e.email.toLowerCase().includes(q)) ||
      (e.username && e.username.toLowerCase().includes(q)) ||
      e.role.toLowerCase().includes(q)
    );
  });

  return (
    <div className="bb-members">
      <div className="bb-members__head">
        <div>
          <h1 className="bb-adminboard__title">{t("nav.Members")}</h1>
          <p className="bb-adminboard__who">
            {businessName} · {rawList.length} {t("screens:admin.employees").toLowerCase()}
          </p>
        </div>
        <div className="bb-members__actions">
          {roleAccess?.isAdmin && (
            <button type="button" className="bibo-btn bibo-btn--secondary bb-members__add" onClick={() => { setWsName(""); setWsError(null); setWsOpen(true); }}>
              <IconPlus /> {newWorkspaceLabel}
            </button>
          )}
          {canCreateMembers && (
            <button type="button" className="bibo-btn bibo-btn--primary bb-members__add" onClick={openModal}>
              <IconPlus /> {t("screens:admin.addMember")}
            </button>
          )}
        </div>
      </div>

      {actionMsg && (
        <div className="auth-success" style={{ background: "var(--accent-weak)", color: "var(--accent)", padding: "8px 14px", borderRadius: 6, marginBottom: 14 }}>
          {actionMsg}
        </div>
      )}

      {listError && <div className="auth-err" role="alert">{listError}</div>}

      {/* Search & Filter Bar */}
      <div style={{ marginBottom: 12 }}>
        <input
          type="text"
          placeholder="Search employees by name, login, or role..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          style={{
            width: "100%",
            maxWidth: 400,
            padding: "8px 12px",
            borderRadius: 6,
            border: "1px solid var(--border)",
            background: "var(--bg-subtle)",
            color: "var(--text)",
            fontSize: 13,
          }}
        />
      </div>

      <div className="card bb-adminboard__tablecard">
        {rows === null ? (
          <div className="muted bb-adminboard__msg">{t("loading")}</div>
        ) : list.length === 0 ? (
          <div className="muted bb-adminboard__msg">
            {search ? "No matching employees found." : t("screens:admin.empty")}
          </div>
        ) : (
          <table className="bb-adminboard__table">
            <thead>
              <tr>
                <th>{t("screens:admin.employees")}</th>
                <th>{t("screens:admin.login")}</th>
                <th>{t("screens:admin.role")}</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {list.map((e, i) => {
                const pal = AVATAR_PALETTE[i % AVATAR_PALETTE.length];
                const status = memberStatus(e.last_seen);
                const isViewerSelf = viewerEmployeeId && e.id === viewerEmployeeId;
                const isOwner = e.role === "owner";
                const isTL = e.role === "team_lead";

                return (
                  <tr key={e.id}>
                    <td>
                      <div className="bb-adminboard__name">
                        <span className="bb-adminboard__avatar" style={{ background: pal.bg, color: pal.fg }}>
                          {initials(e.display_name)}
                          <span className={`bb-adminboard__dot bb-adminboard__dot--${status}`} />
                        </span>
                        <span className="bb-adminboard__nameid">
                          <span className="bb-adminboard__nametxt" title={e.display_name}>{e.display_name}</span>
                          {isOwner && (
                            <span className="bibo-badge bb-adminboard__you" style={{ background: pal.bg, color: pal.fg }}>
                              Owner
                            </span>
                          )}
                          {isViewerSelf && (
                            <span className="bibo-badge" style={{ background: "var(--accent-weak)", color: "var(--accent)" }}>
                              {t("screens:admin.you")}
                            </span>
                          )}
                        </span>
                      </div>
                    </td>
                    <td className="bb-adminboard__login">{e.email || e.username}</td>
                    <td>
                      <span className="bibo-badge" style={{ background: pal.bg, color: pal.fg }}>
                        {isOwner ? "Administrator" : isTL ? "Team Lead" : "Employee"}
                      </span>
                    </td>
                    <td className="r">
                      <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 6 }}>
                        <button type="button" className="bb-adminboard__view" onClick={() => onView(e)}>
                          {t("screens:admin.view")} <IconArrow />
                        </button>

                        {canManageRoles && !isOwner && !isViewerSelf && (
                          isTL ? (
                            <button
                              type="button"
                              className="bb-adminboard__view"
                              style={{ fontSize: 11 }}
                              onClick={() => handleDemote(e)}
                              title="Demote to standard Employee"
                            >
                              Demote
                            </button>
                          ) : (
                            <button
                              type="button"
                              className="bb-adminboard__view"
                              style={{ fontSize: 11, color: "var(--accent)" }}
                              onClick={() => handlePromote(e)}
                              title="Promote to Team Lead"
                            >
                              Promote
                            </button>
                          )
                        )}

                        {canManagePerms && !isOwner && (
                          <button
                            type="button"
                            className="bb-adminboard__view"
                            style={{ fontSize: 11 }}
                            onClick={() => openPermsModal(e)}
                            title="Manage granular RBAC permissions"
                          >
                            <IconShield />
                          </button>
                        )}

                        {roleAccess?.isAdmin && !isOwner && !isViewerSelf && (
                          <>
                            <button
                              type="button"
                              className="bb-adminboard__view"
                              style={{ fontSize: 11, color: "var(--data-amber)" }}
                              onClick={() => handleSuspend(e)}
                              title="Suspend member access"
                            >
                              Suspend
                            </button>
                            <button
                              type="button"
                              className="bb-adminboard__view"
                              style={{ fontSize: 11, color: "var(--data-mint)" }}
                              onClick={() => handleReactivate(e)}
                              title="Reactivate member access"
                            >
                              Reactivate
                            </button>
                            <button
                              type="button"
                              className="bb-adminboard__view"
                              style={{ fontSize: 11, color: "var(--data-rose)" }}
                              onClick={() => handleRemove(e)}
                              title="Remove member from organization"
                            >
                              Remove
                            </button>
                          </>
                        )}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {/* Granular Permissions Modal */}
      {permModalMember && (
        <div className="bb-modal-overlay" onClick={() => !permBusy && setPermModalMember(null)}>
          <div className="bibo-dlg" style={{ maxWidth: 520 }} onClick={(e) => e.stopPropagation()}>
            <div className="bibo-dlg__head">
              <div className="bibo-dlg__icon"><IconShield /></div>
              <div className="bibo-dlg__title">Permissions: {permModalMember.display_name}</div>
              <button type="button" className="bibo-dlg__close" onClick={() => setPermModalMember(null)}>
                <IconX />
              </button>
            </div>
            <div className="bibo-dlg__body">
              <p style={{ fontSize: 12, color: "var(--text-muted)", marginBottom: 12 }}>
                Configure authoritative server-side permissions for this user.
              </p>
              <div style={{ display: "flex", flexDirection: "column", gap: 10, maxHeight: 320, overflowY: "auto" }}>
                {ALL_PERMISSIONS.map((p) => {
                  const granted = memberPerms.includes(p.key);
                  return (
                    <label
                      key={p.key}
                      style={{
                        display: "flex",
                        alignItems: "flex-start",
                        gap: 10,
                        padding: "8px 10px",
                        background: granted ? "var(--accent-weak)" : "var(--bg-subtle)",
                        borderRadius: 6,
                        cursor: "pointer",
                      }}
                    >
                      <input
                        type="checkbox"
                        checked={granted}
                        onChange={(e) => togglePermission(p.key, e.target.checked)}
                        disabled={permBusy}
                        style={{ marginTop: 3 }}
                      />
                      <div>
                        <div style={{ fontWeight: 600, fontSize: 13, color: "var(--text)" }}>{p.label}</div>
                        <div style={{ fontSize: 11, color: "var(--text-muted)" }}>{p.desc}</div>
                      </div>
                    </label>
                  );
                })}
              </div>
            </div>
            <div className="bibo-dlg__foot">
              <button
                type="button"
                className="bibo-btn bibo-btn--primary"
                onClick={() => setPermModalMember(null)}
              >
                Done
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Add Member Modal */}
      {open && (
        <div className="bb-modal-overlay" onClick={() => !busy && closeModal()}>
          <div className="bibo-dlg" onClick={(e) => e.stopPropagation()}>
            {created ? (
              <>
                <div className="bibo-dlg__head">
                  <div className="bibo-dlg__icon"><IconCheck /></div>
                  <div className="bibo-dlg__title">{t("screens:admin.memberCreated")}</div>
                  <button type="button" className="bibo-dlg__close" aria-label={t("screens:admin.done")} onClick={closeModal}>
                    <IconX />
                  </button>
                </div>
                <div className="bibo-dlg__body">
                  <p className="bb-modal__hint">{t("screens:admin.credsHint")}</p>
                  <div className="bibo-dlg__creds">
                    <div><span className="bibo-dlg__credlbl">{t("screens:admin.memberLogin")}</span><span className="num">{created.login}</span></div>
                    <div><span className="bibo-dlg__credlbl">{t("screens:admin.memberPassword")}</span><span className="num">{created.password}</span></div>
                  </div>
                </div>
                <div className="bibo-dlg__foot">
                  <button type="button" className="bibo-btn bibo-btn--ghost" onClick={copyCreds}>
                    <IconCopy /> {copied ? t("screens:admin.copied") : t("screens:admin.copy")}
                  </button>
                  <button type="button" className="bibo-btn bibo-btn--primary" onClick={closeModal}>
                    {t("screens:admin.done")}
                  </button>
                </div>
              </>
            ) : (
              <form onSubmit={submit}>
                <div className="bibo-dlg__head">
                  <div className="bibo-dlg__icon"><IconUserPlus /></div>
                  <div className="bibo-dlg__title">{t("screens:admin.addMember")}</div>
                  <button type="button" className="bibo-dlg__close" aria-label={t("screens:admin.cancel")} onClick={closeModal}>
                    <IconX />
                  </button>
                </div>
                <div className="bibo-dlg__body">
                  {formError && <div className="auth-err" role="alert">{formError}</div>}
                  <label className="bibo-field">
                    <span className="bibo-field__lbl">{t("screens:admin.memberName")}</span>
                    <span className="bibo-input">
                      <input type="text" value={displayName} onChange={(e) => setDisplayName(e.target.value)} autoFocus />
                    </span>
                  </label>
                  <label className="bibo-field">
                    <span className="bibo-field__lbl">{t("screens:admin.memberLogin")}</span>
                    <span className="bibo-input">
                      <input type="text" value={login} onChange={(e) => setLogin(e.target.value)} placeholder="an_nguyen" autoComplete="off" />
                    </span>
                  </label>
                  <label className="bibo-field">
                    <span className="bibo-field__lbl">{t("screens:admin.memberPassword")}</span>
                    <span className="bibo-dlg__pw">
                      <span className="bibo-input">
                        <input type="text" value={password} onChange={(e) => setPassword(e.target.value)} />
                      </span>
                      <button type="button" className="bibo-dlg__gen" onClick={() => setPassword(genPassword())} title={t("screens:admin.generate")}>
                        <IconDice /> {t("screens:admin.generate")}
                      </button>
                    </span>
                  </label>
                </div>
                <div className="bibo-dlg__foot">
                  <button type="button" className="bibo-btn bibo-btn--ghost" onClick={closeModal} disabled={busy}>
                    {t("screens:admin.cancel")}
                  </button>
                  <button type="submit" className="bibo-btn bibo-btn--primary" disabled={busy}>
                    {busy ? t("screens:admin.creating") : t("screens:admin.create")}
                  </button>
                </div>
              </form>
            )}
          </div>
        </div>
      )}

      {/* Create Workspace Modal */}
      {wsOpen && (
        <div className="bb-modal-overlay" onClick={() => !wsBusy && setWsOpen(false)}>
          <div className="bibo-dlg bibo-dlg--org" onClick={(e) => e.stopPropagation()}>
            <div className="bibo-dlg__head">
              <div className="bibo-dlg__icon">{isFamily ? <IconHome /> : <IconTeam />}</div>
              <div className="bibo-dlg__title">{newWorkspaceLabel}</div>
              <button type="button" className="bibo-dlg__close" aria-label={t("screens:admin.cancel")} onClick={() => setWsOpen(false)}>
                <IconX />
              </button>
            </div>
            <form onSubmit={submitWorkspace}>
              <div className="bibo-dlg__body">
                {wsError && <div className="auth-err" role="alert">{wsError}</div>}
                <label className="bibo-field">
                  <span className="bibo-field__lbl">{wsNameLabel}</span>
                  <span className="bibo-input">
                    <input type="text" value={wsName} onChange={(e) => setWsName(e.target.value)} placeholder={wsPlaceholder} autoFocus />
                  </span>
                </label>
              </div>
              <div className="bibo-dlg__foot">
                <button type="button" className="bibo-btn bibo-btn--ghost" onClick={() => setWsOpen(false)} disabled={wsBusy}>
                  {t("screens:admin.cancel")}
                </button>
                <button type="submit" className="bibo-btn bibo-btn--primary" disabled={wsBusy}>
                  {wsBusy ? t("screens:admin.creating") : t("screens:admin.create")}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
