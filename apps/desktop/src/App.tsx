import { useCallback, useEffect, useMemo, useRef, useState, type ReactElement } from "react";
import { call as invoke } from "./api";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { useTranslation } from "react-i18next";
import { Segmented } from "./ui";
import { dragWindow } from "./components/dragWindow";
import { Dashboard } from "./screens/Dashboard";
import { Permissions } from "./screens/Permissions";
import { Browser } from "./screens/Browser";
import { Activity } from "./screens/Activity";
import { Settings, type AppSettings, type CaptureManaged } from "./screens/Settings";
import { Login, type Session } from "./screens/Login";
import { TeamOverview } from "./screens/admin/TeamOverview";
import { Members } from "./screens/admin/Members";
import { Teams } from "./screens/admin/Teams";
import { ScreenshotManagement } from "./screens/admin/ScreenshotManagement";
import { AuditLogs } from "./screens/admin/AuditLogs";
import { WorkspacePicker } from "./screens/admin/WorkspacePicker";
import { type OwnerBusiness, type RosterEntry } from "./screens/admin/AdminDashboard";
import { Welcome } from "./screens/Welcome";
import { Onboarding } from "./screens/Onboarding";
import { LanguageSwitcher } from "./components/LanguageSwitcher";
import { MessageCenter } from "./messages/MessageCenter";
import { AppTrayMenu } from "./components/AppTrayMenu";
import { useRoleAccess } from "./hooks/useRoleAccess";

type Screen =
  | "Dashboard"
  | "Activity"
  | "Browser"
  | "Permissions"
  | "TeamOverview"
  | "Members"
  | "Teams"
  | "ScreenshotManagement"
  | "AuditLogs"
  | "Settings";

// Sidebar is grouped dynamically per role:
// - "me" = Personal tracking (all users)
// - "team" = Team Lead monitoring (assigned teams)
// - "hr" = Organization HR management (all employees)
// - "admin" = Full Organization Administration
// - "app" = Application settings
type NavGroup = { key: "me" | "admin" | "team" | "hr" | "app"; items: Screen[] };

/* ---- sidebar icons (stroke = currentColor, so they follow the nav item color) ---- */
const svgProps = {
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;
const GridIcon = () => (
  <svg {...svgProps} aria-hidden>
    <rect x="3" y="3" width="7" height="7" rx="1.5" /><rect x="14" y="3" width="7" height="7" rx="1.5" />
    <rect x="14" y="14" width="7" height="7" rx="1.5" /><rect x="3" y="14" width="7" height="7" rx="1.5" />
  </svg>
);
const ActivityIcon = () => (
  <svg {...svgProps} aria-hidden><path d="M22 12h-4l-3 9L9 3l-3 9H2" /></svg>
);
const GlobeNavIcon = () => (
  <svg {...svgProps} aria-hidden>
    <circle cx="12" cy="12" r="9" /><path d="M3 12h18" /><path d="M12 3a13 13 0 0 1 0 18 13 13 0 0 1 0-18" />
  </svg>
);
const ShieldNavIcon = () => (
  <svg {...svgProps} aria-hidden><path d="M12 3l7 3v5c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6z" /><path d="M9 12l2 2 4-4" /></svg>
);
const GearIcon = () => (
  <svg {...svgProps} aria-hidden>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06-.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
  </svg>
);
const HardDriveIcon = () => (
  <svg {...svgProps} aria-hidden>
    <path d="M10 16h.01" />
    <path d="M2.212 11.577a2 2 0 0 0-.212.896V18a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-5.527a2 2 0 0 0-.212-.896L18.55 5.11A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
    <path d="M21.946 12.013H2.054" />
    <path d="M6 16h.01" />
  </svg>
);
const UserIcon = () => (
  <svg {...svgProps} aria-hidden><circle cx="12" cy="8" r="4" /><path d="M4 21a8 8 0 0 1 16 0" /></svg>
);
const MembersIcon = () => (
  <svg {...svgProps} aria-hidden>
    <circle cx="9" cy="8" r="3.2" /><path d="M3.5 20a5.5 5.5 0 0 1 11 0" />
    <circle cx="17.5" cy="9.5" r="2.4" /><path d="M15 20a4.5 4.5 0 0 1 6.5-4" />
  </svg>
);
const TeamOverviewIcon = () => (
  <svg {...svgProps} aria-hidden>
    <path d="M3 3v18h18" /><rect x="7" y="10" width="3" height="7" rx="1" />
    <rect x="12" y="6" width="3" height="11" rx="1" /><rect x="17" y="13" width="3" height="4" rx="1" />
  </svg>
);
const TeamsNavIcon = () => (
  <svg {...svgProps} aria-hidden>
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
  </svg>
);
const CameraNavIcon = () => (
  <svg {...svgProps} aria-hidden>
    <path d="M14.5 4h-5L8 6H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-4z" />
    <circle cx="12" cy="13" r="3.2" />
  </svg>
);
const AuditNavIcon = () => (
  <svg {...svgProps} aria-hidden>
    <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
  </svg>
);

const NAV_ICON: Record<Screen, () => ReactElement> = {
  Dashboard: GridIcon,
  Activity: ActivityIcon,
  Browser: GlobeNavIcon,
  Permissions: ShieldNavIcon,
  TeamOverview: TeamOverviewIcon,
  Members: MembersIcon,
  Teams: TeamsNavIcon,
  ScreenshotManagement: CameraNavIcon,
  AuditLogs: AuditNavIcon,
  Settings: GearIcon,
};

/* pause glyph shown inside the header tracking pill when paused ("❚❚ Paused") */
const PauseBars = () => (
  <svg className="bb-trackpill__glyph" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
    <rect x="6" y="4" width="4" height="16" rx="1" />
    <rect x="14" y="4" width="4" height="16" rx="1" />
  </svg>
);

function applyTheme(mode: string) {
  const root = document.documentElement;
  if (mode.toLowerCase() === "system") {
    const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    root.setAttribute("data-theme", dark ? "dark" : "light");
  } else {
    root.setAttribute("data-theme", mode.toLowerCase());
  }
}

type TrackStatus = "tracking" | "idle" | "paused";

type SyncStatus = {
  last_sync_ts: number;
  pending: number;
  last_error: string;
};

type EffectivePolicy = {
  state: "active" | "unavailable" | "loading" | "disabled";
  capture_enabled: boolean;
  continuous_capture_enabled: boolean;
  capture_interval_minutes: number;
  manual_capture_enabled: boolean;
  policy_version: number;
  last_synced_at: number;
  employee_id: string;
};

function App() {
  const { t, i18n } = useTranslation();
  const [screen, setScreen] = useState<Screen>("Dashboard");
  const [status, setStatus] = useState<TrackStatus>("tracking");
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [captureManaged, setCaptureManaged] = useState<CaptureManaged | null>(null);
  const [syncStatus, setSyncStatus] = useState<SyncStatus | null>(null);
  const [policyStatus, setPolicyStatus] = useState<EffectivePolicy | null>(null);
  // undefined = still checking; null = logged out; Session = logged in.
  const [session, setSession] = useState<Session | null | undefined>(undefined);
  // Whether the user clicked "I have an account" on the welcome screen.
  const [showLogin, setShowLogin] = useState(false);
  // Workspaces the signed-in user owns (via `admin_businesses`, scoped to
  // `owner_user_id = caller` server-side). Non-empty ⇒ owner: the admin nav group
  // shows. `bizId` is the workspace the admin screens act on (shared picker).
  const [businesses, setBusinesses] = useState<OwnerBusiness[]>([]);
  const [bizId, setBizId] = useState<string | null>(null);
  // Authoritative server-side role and permission access
  const roleAccess = useRoleAccess(bizId, session);

  // The member being viewed under the Members tab. Set from "View" in either the
  // team overview or the members list; navigating via the sidebar (or switching
  // workspace) clears it back to the roster. Its name replaces the header title.
  const [memberDetail, setMemberDetail] = useState<RosterEntry | null>(null);
  // Installed app version (from tauri.conf.json), shown under the sidebar brand.
  const [version, setVersion] = useState<string>("");

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  useEffect(() => {
    invoke<Session | null>("current_session")
      .then((s) => setSession(s ?? null))
      .catch(() => setSession(null));
    // Settings are local (no auth) — load them up front so the welcome/personal
    // gate can read `local_only` before any login.
    invoke<AppSettings>("get_settings").then(setSettings).catch(() => {});
    // Sync the native side (tray) to the UI's detected/saved language on startup.
    invoke("set_locale", { locale: i18n.resolvedLanguage ?? "en" }).catch(() => {});
  }, [i18n.resolvedLanguage]);

  // Resolve workspaces whenever session changes:
  // - Owners fetch owned businesses via `admin_businesses`
  // - Team leads, HR, and employees resolve their assigned workspace via `session.business_id`
  useEffect(() => {
    if (!session) {
      setBusinesses([]);
      setBizId(null);
      return;
    }
    let alive = true;
    invoke<OwnerBusiness[]>("admin_businesses")
      .then((bs) => {
        if (!alive) return;
        const list = Array.isArray(bs) ? bs : [];
        if (list.length > 0) {
          setBusinesses(list);
          setBizId((cur) => cur ?? list[0]?.id ?? null);
        } else if (session.business_id) {
          invoke<{ id: string; name: string }>("admin_business_detail", {
            businessId: session.business_id,
          })
            .then((b) => {
              if (!alive) return;
              const autoBiz: OwnerBusiness = {
                id: session.business_id!,
                name: b?.name || "Organization",
                kind: "team",
              };
              setBusinesses([autoBiz]);
              setBizId(session.business_id!);
            })
            .catch(() => {
              if (!alive) return;
              const autoBiz: OwnerBusiness = {
                id: session.business_id!,
                name: "Organization",
                kind: "team",
              };
              setBusinesses([autoBiz]);
              setBizId(session.business_id!);
            });
        } else {
          setBusinesses([]);
          setBizId(null);
        }
      })
      .catch(() => {
        if (!alive) return;
        if (session.business_id) {
          const autoBiz: OwnerBusiness = {
            id: session.business_id,
            name: "Organization",
            kind: "team",
          };
          setBusinesses([autoBiz]);
          setBizId(session.business_id);
        } else {
          setBusinesses([]);
          setBizId(null);
        }
      });
    return () => {
      alive = false;
    };
  }, [session]);

  // Dynamically compute permitted navigation groups based on authoritative role
  const navGroups: NavGroup[] = useMemo(() => {
    const groups: NavGroup[] = [
      { key: "me", items: ["Dashboard", "Activity", "Browser", "Permissions"] },
    ];

    if (roleAccess.isAdmin) {
      groups.push({
        key: "admin",
        items: ["TeamOverview", "Members", "Teams", "ScreenshotManagement", "AuditLogs"],
      });
    } else if (roleAccess.isHR) {
      groups.push({
        key: "hr",
        items: ["TeamOverview", "Members", "Teams", "ScreenshotManagement"],
      });
    } else if (roleAccess.isTeamLead) {
      groups.push({
        key: "team",
        items: ["TeamOverview", "Members"],
      });
    }

    groups.push({ key: "app", items: ["Settings"] });
    return groups;
  }, [roleAccess.isAdmin, roleAccess.isHR, roleAccess.isTeamLead]);

  // Route security guard: enforce that current screen is permitted for the active role
  useEffect(() => {
    const allowed = navGroups.flatMap((g) => g.items);
    if (!allowed.includes(screen)) {
      setScreen("Dashboard");
    }
  }, [navGroups, screen]);

  // Switching workspace drops any open member detail (it belongs to the old one).
  useEffect(() => {
    setMemberDetail(null);
  }, [bizId]);

  // Past the auth gate when either signed in OR running in personal/local mode.
  const pastAuthGate = session != null || settings?.local_only === true;

  // Reaching the welcome page re-arms onboarding: whichever path the user takes
  // from there (Only me or sign-in), the intro flow shows again afterwards.
  useEffect(() => {
    if (session === undefined || settings === null) return; // still loading
    if (!pastAuthGate && settings.onboarding_completed) {
      updateSettings({ onboarding_completed: false });
    }
  }, [session, settings, pastAuthGate]);

  // Tracking must not run before setup is complete (BRI-22): pause it on the
  // welcome/login/onboarding surfaces (native tray icon goes red), resume when
  // the user reaches the main app.
  const inSetup = !pastAuthGate || settings?.onboarding_completed === false;
  useEffect(() => {
    if (session === undefined || settings === null) return; // still loading
    invoke("set_in_setup", { inSetup }).catch(() => {});
  }, [session === undefined, settings === null, inSetup]);

  // Startup permission check (ticket: no OS prompts at launch). The app no longer
  // requests permissions on startup; instead, once per launch, if a required
  // permission is missing we explain why and open the Permissions screen so every
  // OS prompt stays user-initiated.
  const [permNotice, setPermNotice] = useState(false);
  const permCheckedRef = useRef(false);
  useEffect(() => {
    if (!pastAuthGate || !settings?.onboarding_completed || permCheckedRef.current) return;
    permCheckedRef.current = true;
    invoke<{ required: boolean; state: string }[]>("permissions_status")
      .then((caps) => {
        if (caps.some((c) => c.required && c.state !== "granted")) {
          setPermNotice(true);
          setScreen("Permissions");
        }
      })
      .catch(() => {});
  }, [pastAuthGate, settings]);

  // While the notice is up, keep re-checking (same cadence as the Permissions
  // screen) and auto-dismiss it once every required permission is granted.
  useEffect(() => {
    if (!permNotice) return;
    const check = () =>
      invoke<{ required: boolean; state: string }[]>("permissions_status")
        .then((caps) => {
          if (!caps.some((c) => c.required && c.state !== "granted")) setPermNotice(false);
        })
        .catch(() => {});
    const id = setInterval(check, 1500);
    return () => clearInterval(id);
  }, [permNotice]);

  // Re-apply the org capture policy and reload settings. Run on login AND whenever
  // the window regains focus, so admin changes show up next time it's reopened
  // (closing the window doesn't unmount the webview, so a one-time effect wouldn't).
  const refreshFromBackend = useCallback(() => {
    invoke<CaptureManaged>("apply_org_policy")
      .then(setCaptureManaged)
      .catch(() => {})
      .finally(() => {
        invoke<AppSettings>("get_settings").then(setSettings).catch(() => {});
      });
  }, []);

  // Tracking status pill works for both signed-in and personal/local users.
  useEffect(() => {
    if (!pastAuthGate) return;
    invoke<TrackStatus>("tracking_state").then(setStatus).catch(() => {});
    // The tray broadcasts tracking / idle / paused — keep the pill in sync with it.
    const unlisten = listen<TrackStatus>("tracking-state", (e) => setStatus(e.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, [pastAuthGate]);

  // Periodic polling for truthful connection & screenshot policy status
  useEffect(() => {
    if (!pastAuthGate) return;
    const poll = () => {
      invoke<SyncStatus>("sync_status").then(setSyncStatus).catch(() => {});
      invoke<EffectivePolicy>("screenshot_policy_status").then(setPolicyStatus).catch(() => {});
    };
    poll();
    const id = setInterval(poll, 4000);
    return () => clearInterval(id);
  }, [pastAuthGate]);

  // Org capture policy only applies to real (signed-in) accounts.
  useEffect(() => {
    if (!session) return;
    refreshFromBackend();
  }, [session, refreshFromBackend]);

  // Refresh when the window is reopened/refocused (menu-bar → Open main UI).
  useEffect(() => {
    if (!session) return;
    const onVisible = () => {
      if (!document.hidden) refreshFromBackend();
    };
    window.addEventListener("focus", refreshFromBackend);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.removeEventListener("focus", refreshFromBackend);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [session, refreshFromBackend]);

  const theme = settings?.theme ?? "System";
  useEffect(() => {
    applyTheme(theme);
    if (theme.toLowerCase() !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => applyTheme("System");
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [theme]);

  async function updateSettings(patch: Partial<AppSettings>) {
    if (!settings) return;
    const next = { ...settings, ...patch };
    setSettings(next);
    try {
      await invoke("set_settings", { value: next });
    } catch {
      /* keep optimistic value; will reconcile on next load */
    }
  }

  async function toggleTracking() {
    // Paused -> resume; tracking or idle -> pause.
    const pause = status !== "paused";
    const prev = status;
    setStatus(pause ? "paused" : "tracking");
    try {
      await invoke("set_paused", { paused: pause });
    } catch {
      setStatus(prev);
    }
  }

  async function signOut() {
    try {
      await invoke("logout");
    } catch {
      /* clear locally regardless */
    }
    // Only drop the session — settings are local (no auth) and are loaded once on
    // mount. Clearing them here left the router stuck on the loading gate
    // (`settings === null`) with nothing to reload them, so logout never reached
    // the welcome/login screen. BRI-21
    setSession(null);
    setScreen("Dashboard");
  }

  // Wait until we know both the session and local settings before routing.
  if (session === undefined || settings === null) {
    return (
      <div className="login">
        <div className="muted">{t("loading")}</div>
      </div>
    );
  }
  // No account and not in personal/local mode → the welcome/persona branch.
  if (!pastAuthGate) {
    return showLogin ? (
      <Login onLoggedIn={setSession} onBack={() => setShowLogin(false)} />
    ) : (
      <Welcome
        onUseLocally={() => updateSettings({ local_only: true })}
        onSignIn={() => setShowLogin(true)}
      />
    );
  }

  // First-run onboarding (3 steps: what's captured → configure → permissions),
  // shown once per install. Step 1 is the "What Finjulia captures" disclosure,
  // so finishing/skipping it also records `consented` (gates capture on Windows).
  if (settings && !settings.onboarding_completed) {
    return (
      <Onboarding
        settings={settings}
        captureManaged={captureManaged}
        onChange={updateSettings}
        onFinish={() => updateSettings({ onboarding_completed: true, consented: true })}
      />
    );
  }

  const isManagementScreen =
    screen === "TeamOverview" ||
    screen === "Members" ||
    screen === "Teams" ||
    screen === "ScreenshotManagement" ||
    screen === "AuditLogs";
  // On the Members tab, a drilled-in member's name replaces the section title.
  const headerTitle =
    screen === "Members" && memberDetail ? memberDetail.display_name : t(`nav.${screen}`);
  const trackClass =
    status === "paused" ? "is-paused" : status === "idle" ? "is-idle" : "is-tracking";
  const pillTitle =
    status === "paused"
      ? t("statusTooltip.paused")
      : status === "idle"
        ? t("statusTooltip.idle")
        : t("statusTooltip.tracking");

  const connectionState = settings?.local_only
    ? "local"
    : !session
      ? "offline"
      : syncStatus?.last_error
        ? "error"
        : "connected";

  return (
    <div className="app">
      <div className="app-titlebar" onMouseDown={dragWindow}>
        <span className="app-titlebar-title">Finjulia — {headerTitle}</span>
        <AppTrayMenu status={status} onToggleTracking={toggleTracking} />
      </div>
      <div className="app-body">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-logo" aria-hidden>
            <svg viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="9" />
              <path d="M4.5 12 h3.2 l1.8 -4.4 l2.4 8.8 l1.8 -4.4 h4.5" />
            </svg>
          </span>
          <span className="brand-text">
            <span className="brand-name">Finjulia</span>
            {version && <span className="brand-version">v{version}</span>}
          </span>
        </div>
        <nav className="nav">
          {navGroups.map((g) => (
            <div key={g.key} className={`nav-group nav-group--${g.key}`}>
              {g.key !== "app" && (
                <div className="nav-group__label">{t(`nav.group.${g.key}`)}</div>
              )}
              {g.items.map((n) => {
                const Ic = NAV_ICON[n];
                return (
                  <div
                    key={n}
                    className={`nav-item ${screen === n ? "active" : ""}`}
                    onClick={() => {
                      setScreen(n);
                      setMemberDetail(null);
                    }}
                  >
                    <span className="nav-ic"><Ic /></span>
                    {t(`nav.${n}`)}
                  </div>
                );
              })}
            </div>
          ))}
        </nav>
        <div className="sidebar-foot">
          <div
            className={`side-status ${status === "paused" ? "paused" : status === "idle" ? "idle" : ""}`}
            title={t(`statusTooltip.${status}`)}
          >
            <span className="dot" /> {t(`status.${status}`)}
          </div>
          {session ? (
            <div className="account-box">
              <div className="account-row">
                <span className="account-ic"><UserIcon /></span>
                <div style={{ display: "flex", flexDirection: "column", minWidth: 0, flex: 1 }}>
                  <span className="account-text" style={{ fontWeight: 600, color: "var(--text)" }} title={session.display_name || session.email}>
                    {session.display_name || session.email}
                  </span>
                  {session.display_name && session.email && (
                    <span className="account-text" style={{ fontSize: 11, color: "var(--text-muted)" }} title={session.email}>
                      {session.email}
                    </span>
                  )}
                  <span
                    className="bb-role-badge"
                    style={{
                      background: roleAccess.isAdmin
                        ? "var(--accent)"
                        : roleAccess.isHR
                          ? "var(--data-mint)"
                          : roleAccess.isTeamLead
                            ? "var(--data-amber)"
                            : "var(--bg-subtle)",
                      color: roleAccess.isAdmin || roleAccess.isHR || roleAccess.isTeamLead
                        ? "#000"
                        : "var(--text)",
                      fontWeight: 600,
                      textTransform: "uppercase",
                      fontSize: 10,
                      padding: "2px 8px",
                      borderRadius: 12,
                      alignSelf: "flex-start",
                      marginTop: 4,
                    }}
                  >
                    {roleAccess.role === "admin"
                      ? "Administrator"
                      : roleAccess.role === "hr"
                        ? "HR Manager"
                        : roleAccess.role === "team_lead"
                          ? "Team Lead"
                          : "Employee"}
                  </span>
                </div>
              </div>
              <button className="account-link" onClick={signOut}>
                {t("account.signOut")}
              </button>
            </div>
          ) : (
            <div className="account-box">
              <div className="account-row" title={t("account.localTooltip")}>
                <span className="account-ic"><HardDriveIcon /></span>
                <span className="account-text">{t("account.local")}</span>
              </div>
              <button
                className="account-link"
                onClick={() => {
                  setShowLogin(true);
                  updateSettings({ local_only: false });
                }}
                title={t("account.setupAgainTooltip")}
              >
                {t("account.setupAgain")} <span aria-hidden>→</span>
              </button>
            </div>
          )}
        </div>
      </aside>

      <div className="main">
        <header className="header">
          <h1>{headerTitle}</h1>
          <div className="header-right">
            {isManagementScreen && businesses.length > 0 && (
              <WorkspacePicker
                businesses={businesses}
                bizId={bizId}
                onSelect={setBizId}
              />
            )}

            {/* Truthful Connection Status */}
            <span className="bibo-tip">
              <span className="bb-statuspill">
                <span
                  className={`bb-statuspill__dot ${
                    connectionState === "connected"
                      ? "is-green"
                      : connectionState === "error"
                        ? "is-red"
                        : "is-amber"
                  }`}
                />
                <span>
                  {connectionState === "connected"
                    ? t("connection.connected", "Connected")
                    : connectionState === "error"
                      ? t("connection.syncIssue", "Sync issue")
                      : connectionState === "local"
                        ? t("connection.localMode", "Local mode")
                        : t("connection.offline", "Offline")}
                </span>
              </span>
              <span className="bibo-tip__bubble" role="tooltip">
                {connectionState === "connected"
                  ? "Connected to server · Sync active"
                  : connectionState === "error"
                    ? syncStatus?.last_error
                      ? `Sync failed: ${syncStatus.last_error}`
                      : "Sync error encountered"
                    : connectionState === "local"
                      ? "Running in offline local mode"
                      : "Not connected"}
              </span>
            </span>

            {/* Truthful Screenshot Monitoring Status */}
            {session && (
              <span className="bibo-tip">
                <span className="bb-statuspill">
                  <span
                    className={`bb-statuspill__dot ${
                      policyStatus?.state === "active" && policyStatus?.capture_enabled
                        ? "is-green"
                        : (policyStatus?.state === "unavailable" || policyStatus?.state === "loading")
                          ? "is-amber"
                          : "is-neutral"
                    }`}
                  />
                  <span>
                    {policyStatus?.state === "active" && policyStatus?.capture_enabled
                      ? t("monitoring.active", "Screenshots: On")
                      : (policyStatus?.state === "unavailable" || policyStatus?.state === "loading")
                        ? t("monitoring.suspended", "Screenshots: Suspended")
                        : t("monitoring.off", "Screenshots: Off")}
                  </span>
                </span>
                <span className="bibo-tip__bubble" role="tooltip">
                  {policyStatus?.state === "active" && policyStatus?.capture_enabled
                    ? "Screenshot monitoring active per organization policy"
                    : (policyStatus?.state === "unavailable" || policyStatus?.state === "loading")
                      ? "Screenshot capture temporarily suspended (fail-closed)"
                      : "Screenshot capture disabled by organization policy"}
                </span>
              </span>
            )}

            <LanguageSwitcher compact />
            <Segmented
              options={["Light", "Dark", "System"]}
              value={theme}
              labels={{
                Light: t("theme.light"),
                Dark: t("theme.dark"),
                System: t("theme.system"),
              }}
              onChange={(v) => updateSettings({ theme: v })}
            />
            <span className="bibo-tip">
              <button
                className={`bb-trackpill ${trackClass}`}
                onClick={toggleTracking}
                aria-label={pillTitle}
              >
                {status === "paused" ? <PauseBars /> : <span className="bb-trackpill__dot" />}
                {t(`status.${status}`)}
              </button>
              <span className="bibo-tip__bubble" role="tooltip">{pillTitle}</span>
            </span>
          </div>
        </header>

        <main className="content">
          {roleAccess.isSuspended && (
            <div
              className="perm-notice"
              role="alert"
              style={{
                borderColor: "var(--data-rose)",
                background: "color-mix(in srgb, var(--data-rose) 12%, var(--bg-card))",
                marginBottom: 16,
              }}
            >
              <div className="perm-notice__text">
                <strong style={{ color: "var(--data-rose)" }}>Organization Membership Suspended</strong>
                <span>Your organization account has been suspended by an administrator. Management capabilities are restricted.</span>
              </div>
            </div>
          )}
          {permNotice && (
            <div className="perm-notice" role="alert">
              <div className="perm-notice__text">
                <strong>{t("permissions:startupNotice.title")}</strong>
                <span>{t("permissions:startupNotice.body")}</span>
                {screen !== "Permissions" && (
                  <button className="perm-notice__link" onClick={() => setScreen("Permissions")}>
                    {t("permissions:startupNotice.open")} <span aria-hidden>→</span>
                  </button>
                )}
              </div>
              <button
                className="perm-notice__close"
                aria-label={t("permissions:startupNotice.dismiss")}
                onClick={() => setPermNotice(false)}
              >
                ×
              </button>
            </div>
          )}
          {screen === "Dashboard" && <Dashboard />}
          {screen === "Activity" && <Activity />}
          {screen === "Browser" && <Browser />}
          {screen === "Permissions" && <Permissions />}
          {screen === "TeamOverview" &&
            (bizId ? (
              <TeamOverview
                businessId={bizId}
                businessName={businesses.find((b) => b.id === bizId)?.name ?? ""}
                onViewEmployee={(e) => {
                  setMemberDetail(e);
                  setScreen("Members");
                }}
              />
            ) : (
              <div className="muted bb-adminboard__msg">{t("loading")}</div>
            ))}
          {screen === "Members" &&
            (bizId ? (
              <Members
                businessId={bizId}
                businessName={businesses.find((b) => b.id === bizId)?.name ?? ""}
                businessKind={businesses.find((b) => b.id === bizId)?.kind ?? "team"}
                onWorkspaceCreated={(biz) => {
                  setBusinesses((prev) => [...prev, biz]);
                  setBizId(biz.id);
                }}
                detail={memberDetail}
                onView={setMemberDetail}
                roleAccess={roleAccess}
                viewerEmployeeId={session?.employee_id}
              />
            ) : (
              <div className="muted bb-adminboard__msg">{t("loading")}</div>
            ))}
          {screen === "Teams" &&
            (bizId ? (
              <Teams
                businessId={bizId}
                canManageTeams={roleAccess.isAdmin || roleAccess.can("can_manage_team")}
              />
            ) : (
              <div className="muted bb-adminboard__msg">{t("loading")}</div>
            ))}
          {screen === "ScreenshotManagement" &&
            (bizId ? (
              <ScreenshotManagement
                businessId={bizId}
                canManage={roleAccess.isAdmin || roleAccess.can("can_manage_screenshot_policy")}
              />
            ) : (
              <div className="muted bb-adminboard__msg">{t("loading")}</div>
            ))}
          {screen === "AuditLogs" &&
            (bizId ? (
              <AuditLogs businessId={bizId} />
            ) : (
              <div className="muted bb-adminboard__msg">{t("loading")}</div>
            ))}
          {screen === "Settings" && (
            <Settings
              settings={settings}
              captureManaged={captureManaged}
              onChange={updateSettings}
              onOpenPermissions={() => setScreen("Permissions")}
            />
          )}
        </main>
      </div>
      </div>
      <MessageCenter user={session && !settings.local_only ? session.email : null} version={version} />
    </div>
  );
}

export default App;
