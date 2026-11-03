// Enterprise RBAC & Management Types for BiBo / Finjulia Desktop Client

export type CanonicalRole = "owner" | "team_lead" | "employee";

export type EffectiveAppRole = "admin" | "hr" | "team_lead" | "employee";

export type RbacPermission =
  | "can_view_team"
  | "can_manage_employees"
  | "can_create_team_leads"
  | "can_assign_employees"
  | "can_manage_screenshot_policy"
  | "can_view_screenshots"
  | "can_view_reports"
  | "can_manage_team"
  | "can_view_audit_logs";

export type MyAccess = {
  role: CanonicalRole;
  is_org_wide: boolean;
  permissions: string[];
  authorized_team_ids: string[];
  business_name?: string;
  business_id?: string;
  is_suspended?: boolean;
};

export type OrgMember = {
  id: string;
  user_id: string;
  business_id: string;
  role: CanonicalRole;
  display_name: string;
  email: string;
  username: string;
  status: "active" | "suspended" | "pending";
  team_id?: string | null;
  team_name?: string | null;
  created_at?: string;
  last_seen?: number | null;
};

export type TeamWithDetails = {
  id: string;
  business_id: string;
  name: string;
  description?: string;
  parent_team_id?: string | null;
  subteam_count?: number;
  member_count?: number;
  lead_name?: string | null;
  lead_user_id?: string | null;
  created_at?: string;
  members?: OrgMember[];
};

export type OrgAuditLog = {
  id: string;
  business_id: string;
  actor_user_id: string;
  actor_name: string;
  action: string;
  target_type: string;
  target_id: string;
  target_name?: string;
  metadata?: Record<string, unknown>;
  created_at: string;
};

export type OrgScreenshotSettings = {
  business_id: string;
  capture_enabled: boolean;
  continuous_capture_enabled: boolean;
  capture_interval_minutes: number;
  manual_capture_enabled: boolean;
  updated_at?: string;
};

export type EmployeePolicyOverride = {
  id?: string;
  business_id: string;
  employee_id: string;
  capture_enabled?: boolean | null;
  continuous_capture_enabled?: boolean | null;
  capture_interval_minutes?: number | null;
  manual_capture_enabled?: boolean | null;
  updated_at?: string;
};

export type ManualScreenshotRequest = {
  id: string;
  business_id: string;
  employee_id: string;
  requester_user_id: string;
  requester_name?: string;
  status: "pending" | "fulfilled" | "expired" | "failed";
  created_at: string;
  fulfilled_at?: string | null;
  expires_at: string;
  retry_count?: number;
};
