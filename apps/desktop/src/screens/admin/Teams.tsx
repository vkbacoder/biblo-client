import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { call as invoke } from "../../api";
import type { OrgMember, TeamWithDetails } from "./rbacTypes";

const svgp = { viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round", strokeLinejoin: "round" } as const;
const IconPlus = () => (<svg {...svgp} width="16" height="16" aria-hidden><path d="M5 12h14" /><path d="M12 5v14" /></svg>);
const IconFolder = () => (<svg {...svgp} width="18" height="18" aria-hidden><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" /></svg>);
const IconUserPlus = () => (<svg {...svgp} width="16" height="16" aria-hidden><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M19 8v6M22 11h-6" /></svg>);
const IconTrash = () => (<svg {...svgp} width="15" height="15" aria-hidden><polyline points="3 6 5 6 21 6" /><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" /></svg>);
const IconEdit = () => (<svg {...svgp} width="15" height="15" aria-hidden><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" /><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" /></svg>);
const IconX = () => (<svg {...svgp} width="18" height="18" aria-hidden><path d="M18 6 6 18M6 6l12 12" /></svg>);

export function Teams({
  businessId,
  canManageTeams = true,
}: {
  businessId: string;
  canManageTeams?: boolean;
}) {
  const { t } = useTranslation();
  const [teams, setTeams] = useState<TeamWithDetails[] | null>(null);
  const [members, setMembers] = useState<OrgMember[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Selected team for member management
  const [selectedTeam, setSelectedTeam] = useState<TeamWithDetails | null>(null);
  const [teamMembers, setTeamMembers] = useState<OrgMember[]>([]);
  const [teamMembersLoading, setTeamMembersLoading] = useState(false);

  // Modals
  const [createOpen, setCreateOpen] = useState(false);
  const [editOpen, setEditOpen] = useState(false);
  const [assignOpen, setAssignOpen] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [parentId, setParentId] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [selectedUserId, setSelectedUserId] = useState("");

  const loadData = useCallback(async () => {
    if (!businessId) return;
    setLoading(true);
    setError(null);
    try {
      const [tList, mList] = await Promise.all([
        invoke<TeamWithDetails[] | { teams: TeamWithDetails[] }>("admin_teams", { businessId }),
        invoke<OrgMember[] | { members: OrgMember[] }>("admin_members", { businessId }).catch(() => []),
      ]);
      const safeTeams = Array.isArray(tList) ? tList : (tList as any)?.teams ?? [];
      const safeMembers = Array.isArray(mList) ? mList : (mList as any)?.members ?? [];
      setTeams(safeTeams);
      setMembers(safeMembers);
      if (selectedTeam) {
        const updated = safeTeams.find((item: TeamWithDetails) => item.id === selectedTeam.id);
        if (updated) setSelectedTeam(updated);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [businessId, selectedTeam?.id]);

  useEffect(() => {
    loadData();
  }, [businessId]);

  const loadTeamMembers = useCallback(async (teamId: string) => {
    setTeamMembersLoading(true);
    try {
      const res = await invoke<OrgMember[] | { members: OrgMember[] }>("admin_team_members", {
        businessId,
        teamId,
      });
      const list = Array.isArray(res) ? res : (res as any)?.members ?? [];
      setTeamMembers(list);
    } catch {
      setTeamMembers([]);
    } finally {
      setTeamMembersLoading(false);
    }
  }, [businessId]);

  useEffect(() => {
    if (selectedTeam) {
      loadTeamMembers(selectedTeam.id);
    } else {
      setTeamMembers([]);
    }
  }, [selectedTeam, loadTeamMembers]);

  async function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim() || busy) return;
    setBusy(true);
    try {
      await invoke("admin_create_team", {
        businessId,
        body: {
          name: name.trim(),
          description: description.trim() || undefined,
          parent_team_id: parentId.trim() || null,
        },
      });
      setCreateOpen(false);
      setName("");
      setDescription("");
      setParentId("");
      await loadData();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleUpdate(e: React.FormEvent) {
    e.preventDefault();
    if (!selectedTeam || !name.trim() || busy) return;
    setBusy(true);
    try {
      await invoke("admin_update_team", {
        businessId,
        teamId: selectedTeam.id,
        body: {
          name: name.trim(),
          description: description.trim() || undefined,
          parent_team_id: parentId.trim() || null,
        },
      });
      setEditOpen(false);
      await loadData();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete(team: TeamWithDetails) {
    if (!window.confirm(t("screens:teams.confirmDelete", `Are you sure you want to delete team "${team.name}"?`))) {
      return;
    }
    setBusy(true);
    try {
      await invoke("admin_delete_team", { businessId, teamId: team.id });
      if (selectedTeam?.id === team.id) setSelectedTeam(null);
      await loadData();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleAssignMember(e: React.FormEvent) {
    e.preventDefault();
    if (!selectedTeam || !selectedUserId || busy) return;
    setBusy(true);
    try {
      await invoke("admin_assign_team_member", {
        businessId,
        teamId: selectedTeam.id,
        userId: selectedUserId,
      });
      setAssignOpen(false);
      setSelectedUserId("");
      await loadTeamMembers(selectedTeam.id);
      await loadData();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleRemoveMember(userId: string) {
    if (!selectedTeam || busy) return;
    try {
      await invoke("admin_remove_team_member", {
        businessId,
        teamId: selectedTeam.id,
        userId,
      });
      await loadTeamMembers(selectedTeam.id);
      await loadData();
    } catch (err) {
      setError(String(err));
    }
  }

  const teamList = teams ?? [];

  return (
    <div className="bb-members">
      <div className="bb-members__head">
        <div>
          <h1 className="bb-adminboard__title">{t("screens:teams.title", "Teams Management")}</h1>
          <p className="bb-adminboard__who">
            {teamList.length} {t("screens:teams.count", "teams configured")}
          </p>
        </div>
        {canManageTeams && (
          <div className="bb-members__actions">
            <button
              type="button"
              className="bibo-btn bibo-btn--primary bb-members__add"
              onClick={() => {
                setName("");
                setDescription("");
                setParentId("");
                setCreateOpen(true);
              }}
            >
              <IconPlus /> {t("screens:teams.createTeam", "Create Team")}
            </button>
          </div>
        )}
      </div>

      {error && <div className="auth-err" role="alert">{error}</div>}

      <div style={{ display: "grid", gridTemplateColumns: selectedTeam ? "1fr 1fr" : "1fr", gap: 16 }}>
        {/* Teams List */}
        <div className="card bb-adminboard__tablecard">
          <div style={{ padding: "16px 20px 8px 20px", display: "flex", alignItems: "center", justifyContent: "space-between" }}>
            <span style={{ fontWeight: 600, color: "var(--text)" }}>{t("screens:teams.allTeams", "All Teams")}</span>
            <span style={{ fontSize: 12, color: "var(--text-muted)" }}>{teamList.length} items</span>
          </div>
          {loading && teams === null ? (
            <div className="muted bb-adminboard__msg">{t("loading")}</div>
          ) : teamList.length === 0 ? (
            <div className="muted bb-adminboard__msg">{t("screens:teams.empty", "No teams found. Create your first team.")}</div>
          ) : (
            <table className="bb-adminboard__table">
              <thead>
                <tr>
                  <th>{t("screens:teams.name", "Team")}</th>
                  <th>{t("screens:teams.lead", "Team Lead")}</th>
                  <th>{t("screens:teams.members", "Members")}</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {teamList.map((tm) => {
                  const isSelected = selectedTeam?.id === tm.id;
                  return (
                    <tr
                      key={tm.id}
                      style={{ cursor: "pointer", background: isSelected ? "var(--accent-weak)" : undefined }}
                      onClick={() => setSelectedTeam(tm)}
                    >
                      <td>
                        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                          <span style={{ color: "var(--accent)" }}><IconFolder /></span>
                          <div>
                            <div style={{ fontWeight: 600, color: "var(--text)" }}>{tm.name}</div>
                            {tm.description && (
                              <div style={{ fontSize: 12, color: "var(--text-muted)" }}>{tm.description}</div>
                            )}
                          </div>
                        </div>
                      </td>
                      <td style={{ fontSize: 13, color: "var(--text)" }}>
                        {tm.lead_name || <span className="muted">{t("none", "None")}</span>}
                      </td>
                      <td style={{ fontSize: 13, color: "var(--text)" }}>
                        <span className="bibo-badge" style={{ background: "var(--bg-subtle)", color: "var(--text)" }}>
                          {tm.member_count ?? 0} {t("screens:teams.membersCount", "members")}
                        </span>
                      </td>
                      <td className="r" onClick={(e) => e.stopPropagation()}>
                        {canManageTeams && (
                          <div style={{ display: "flex", gap: 6, justifyContent: "flex-end" }}>
                            <button
                              type="button"
                              className="bb-adminboard__view"
                              title={t("edit", "Edit")}
                              onClick={() => {
                                setSelectedTeam(tm);
                                setName(tm.name);
                                setDescription(tm.description ?? "");
                                setParentId(tm.parent_team_id ?? "");
                                setEditOpen(true);
                              }}
                            >
                              <IconEdit />
                            </button>
                            <button
                              type="button"
                              className="bb-adminboard__view"
                              style={{ color: "var(--data-rose)" }}
                              title={t("delete", "Delete")}
                              onClick={() => handleDelete(tm)}
                            >
                              <IconTrash />
                            </button>
                          </div>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>

        {/* Selected Team Members Panel */}
        {selectedTeam && (
          <div className="card bb-adminboard__tablecard">
            <div style={{ padding: "16px 20px", display: "flex", alignItems: "center", justifyContent: "space-between", borderBottom: "1px solid var(--border)" }}>
              <div>
                <div style={{ fontWeight: 600, color: "var(--text)" }}>{selectedTeam.name} — Members</div>
                <div style={{ fontSize: 12, color: "var(--text-muted)" }}>{selectedTeam.description || "Assigned team roster"}</div>
              </div>
              {canManageTeams && (
                <button
                  type="button"
                  className="bibo-btn bibo-btn--secondary"
                  style={{ padding: "4px 10px", fontSize: 12 }}
                  onClick={() => {
                    setSelectedUserId("");
                    setAssignOpen(true);
                  }}
                >
                  <IconUserPlus /> {t("screens:teams.assignMember", "Add Member")}
                </button>
              )}
            </div>

            {teamMembersLoading ? (
              <div className="muted bb-adminboard__msg">{t("loading")}</div>
            ) : teamMembers.length === 0 ? (
              <div className="muted bb-adminboard__msg">{t("screens:teams.noMembers", "No members assigned to this team.")}</div>
            ) : (
              <table className="bb-adminboard__table">
                <thead>
                  <tr>
                    <th>{t("screens:admin.employees", "Member")}</th>
                    <th>{t("screens:admin.role", "Role")}</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {teamMembers.map((m) => (
                    <tr key={m.id || m.user_id}>
                      <td>
                        <div style={{ fontWeight: 600, color: "var(--text)" }}>{m.display_name || m.username}</div>
                        <div style={{ fontSize: 11, color: "var(--text-muted)" }}>{m.email || m.username}</div>
                      </td>
                      <td>
                        <span className="bibo-badge">
                          {m.role === "team_lead" ? "Team Lead" : "Employee"}
                        </span>
                      </td>
                      <td className="r">
                        {canManageTeams && (
                          <button
                            type="button"
                            className="bb-adminboard__view"
                            style={{ color: "var(--data-rose)" }}
                            title={t("screens:teams.removeMember", "Remove from team")}
                            onClick={() => handleRemoveMember(m.user_id || m.id)}
                          >
                            <IconTrash />
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}
      </div>

      {/* Create Team Modal */}
      {createOpen && (
        <div className="bb-modal-overlay" onClick={() => !busy && setCreateOpen(false)}>
          <div className="bibo-dlg" onClick={(e) => e.stopPropagation()}>
            <form onSubmit={handleCreate}>
              <div className="bibo-dlg__head">
                <div className="bibo-dlg__icon"><IconFolder /></div>
                <div className="bibo-dlg__title">{t("screens:teams.createTeam", "Create Team")}</div>
                <button type="button" className="bibo-dlg__close" onClick={() => setCreateOpen(false)}>
                  <IconX />
                </button>
              </div>
              <div className="bibo-dlg__body">
                <label className="bibo-field">
                  <span className="bibo-field__lbl">{t("screens:teams.teamName", "Team Name")} *</span>
                  <span className="bibo-input">
                    <input type="text" value={name} onChange={(e) => setName(e.target.value)} required autoFocus placeholder="Engineering" />
                  </span>
                </label>
                <label className="bibo-field">
                  <span className="bibo-field__lbl">{t("screens:teams.description", "Description")}</span>
                  <span className="bibo-input">
                    <input type="text" value={description} onChange={(e) => setDescription(e.target.value)} placeholder="Core product engineering team" />
                  </span>
                </label>
                {teamList.length > 0 && (
                  <label className="bibo-field">
                    <span className="bibo-field__lbl">{t("screens:teams.parentTeam", "Parent Team (Optional)")}</span>
                    <span className="bibo-input">
                      <select value={parentId} onChange={(e) => setParentId(e.target.value)}>
                        <option value="">{t("screens:teams.noParent", "None (Top level)")}</option>
                        {teamList.map((tm) => (
                          <option key={tm.id} value={tm.id}>{tm.name}</option>
                        ))}
                      </select>
                    </span>
                  </label>
                )}
              </div>
              <div className="bibo-dlg__foot">
                <button type="button" className="bibo-btn bibo-btn--ghost" onClick={() => setCreateOpen(false)} disabled={busy}>
                  {t("cancel", "Cancel")}
                </button>
                <button type="submit" className="bibo-btn bibo-btn--primary" disabled={busy}>
                  {busy ? t("saving", "Saving...") : t("create", "Create")}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Edit Team Modal */}
      {editOpen && selectedTeam && (
        <div className="bb-modal-overlay" onClick={() => !busy && setEditOpen(false)}>
          <div className="bibo-dlg" onClick={(e) => e.stopPropagation()}>
            <form onSubmit={handleUpdate}>
              <div className="bibo-dlg__head">
                <div className="bibo-dlg__icon"><IconEdit /></div>
                <div className="bibo-dlg__title">{t("screens:teams.editTeam", "Edit Team")}</div>
                <button type="button" className="bibo-dlg__close" onClick={() => setEditOpen(false)}>
                  <IconX />
                </button>
              </div>
              <div className="bibo-dlg__body">
                <label className="bibo-field">
                  <span className="bibo-field__lbl">{t("screens:teams.teamName", "Team Name")} *</span>
                  <span className="bibo-input">
                    <input type="text" value={name} onChange={(e) => setName(e.target.value)} required autoFocus />
                  </span>
                </label>
                <label className="bibo-field">
                  <span className="bibo-field__lbl">{t("screens:teams.description", "Description")}</span>
                  <span className="bibo-input">
                    <input type="text" value={description} onChange={(e) => setDescription(e.target.value)} />
                  </span>
                </label>
              </div>
              <div className="bibo-dlg__foot">
                <button type="button" className="bibo-btn bibo-btn--ghost" onClick={() => setEditOpen(false)} disabled={busy}>
                  {t("cancel", "Cancel")}
                </button>
                <button type="submit" className="bibo-btn bibo-btn--primary" disabled={busy}>
                  {busy ? t("saving", "Saving...") : t("save", "Save Changes")}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Assign Member Modal */}
      {assignOpen && selectedTeam && (
        <div className="bb-modal-overlay" onClick={() => !busy && setAssignOpen(false)}>
          <div className="bibo-dlg" onClick={(e) => e.stopPropagation()}>
            <form onSubmit={handleAssignMember}>
              <div className="bibo-dlg__head">
                <div className="bibo-dlg__icon"><IconUserPlus /></div>
                <div className="bibo-dlg__title">Assign Member to {selectedTeam.name}</div>
                <button type="button" className="bibo-dlg__close" onClick={() => setAssignOpen(false)}>
                  <IconX />
                </button>
              </div>
              <div className="bibo-dlg__body">
                <label className="bibo-field">
                  <span className="bibo-field__lbl">Select Employee</span>
                  <span className="bibo-input">
                    <select
                      value={selectedUserId}
                      onChange={(e) => setSelectedUserId(e.target.value)}
                      required
                    >
                      <option value="">-- Choose member --</option>
                      {members
                        .filter((m) => !teamMembers.some((tm) => (tm.user_id || tm.id) === (m.user_id || m.id)))
                        .map((m) => (
                          <option key={m.user_id || m.id} value={m.user_id || m.id}>
                            {m.display_name} ({m.email || m.username})
                          </option>
                        ))}
                    </select>
                  </span>
                </label>
              </div>
              <div className="bibo-dlg__foot">
                <button type="button" className="bibo-btn bibo-btn--ghost" onClick={() => setAssignOpen(false)} disabled={busy}>
                  {t("cancel", "Cancel")}
                </button>
                <button type="submit" className="bibo-btn bibo-btn--primary" disabled={busy || !selectedUserId}>
                  {busy ? t("saving", "Saving...") : t("assign", "Assign")}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
