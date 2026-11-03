import { useCallback, useEffect, useState } from "react";
import { call as invoke } from "../api";
import type { EffectiveAppRole, MyAccess, RbacPermission } from "../screens/admin/rbacTypes";

export type SessionLike = {
  role?: string;
  account_type?: string;
  employee_id?: string | null;
  business_id?: string | null;
  email?: string;
  display_name?: string;
};

export type RoleAccessState = {
  loading: boolean;
  error: string | null;
  access: MyAccess | null;
  role: EffectiveAppRole;
  isOwner: boolean;
  isAdmin: boolean;
  isHR: boolean;
  isTeamLead: boolean;
  isEmployee: boolean;
  isSuspended: boolean;
  authorizedTeamIds: string[];
  can: (permission: RbacPermission) => boolean;
  refresh: () => Promise<void>;
};

export function useRoleAccess(
  businessId: string | null,
  session: SessionLike | null | undefined,
): RoleAccessState {
  const [access, setAccess] = useState<MyAccess | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchAccess = useCallback(async () => {
    if (!businessId || !session) {
      setAccess(null);
      setLoading(false);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const res = await invoke<MyAccess>("admin_my_access", { businessId });
      setAccess(res);
    } catch (err) {
      // If backend returns an error or user lacks admin_my_access endpoint rights,
      // fallback to session role safely
      setError(String(err));
      setAccess(null);
    } finally {
      setLoading(false);
    }
  }, [businessId, session]);

  useEffect(() => {
    fetchAccess();
  }, [fetchAccess]);

  // Determine effective application role from authoritative server access and session
  let role: EffectiveAppRole = "employee";

  const sessRole = (session?.role || session?.account_type || "").toLowerCase();
  const serverRole = access?.role?.toLowerCase();

  if (serverRole === "owner" || sessRole === "owner" || sessRole === "manager") {
    role = "admin";
  } else if (
    serverRole === "team_lead" ||
    sessRole === "team_lead" ||
    sessRole === "teamlead"
  ) {
    role = "team_lead";
  } else if (
    (access?.is_org_wide &&
      (access.permissions?.includes("can_view_reports") ||
        access.permissions?.includes("can_manage_employees"))) ||
    sessRole === "hr" ||
    (access?.permissions?.includes("can_view_reports") &&
      access?.permissions?.includes("can_view_screenshots") &&
      !access?.authorized_team_ids?.length)
  ) {
    role = "hr";
  } else {
    role = "employee";
  }

  const isAdmin = role === "admin";
  const isHR = role === "hr";
  const isTeamLead = role === "team_lead";
  const isEmployee = role === "employee";
  const isOwner = isAdmin;
  const isSuspended = access?.is_suspended === true;
  const authorizedTeamIds = access?.authorized_team_ids ?? [];

  const can = useCallback(
    (permission: RbacPermission): boolean => {
      if (isAdmin) return true;
      if (!access) return false;
      return Array.isArray(access.permissions) && access.permissions.includes(permission);
    },
    [isAdmin, access],
  );

  return {
    loading,
    error,
    access,
    role,
    isOwner,
    isAdmin,
    isHR,
    isTeamLead,
    isEmployee,
    isSuspended,
    authorizedTeamIds,
    can,
    refresh: fetchAccess,
  };
}
