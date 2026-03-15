import { useState, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { Routes, Route } from "react-router-dom";
import Login from "./Login";
import { fetchProjects } from "./services/auth";
import "./App.css";
import { IdleWindow } from "./components/IdleWindow";
import { QuitWindow } from "./components/QuitWindow";
import { PermissionsModal } from "./components/PermissionsModal";
import { BreakModal } from "./components/BreakModal";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

interface Project {
  id: string;
  name: string;
  weeklyLimitHours: number | null;
  dailyLimitHours: number | null;
  screenshotsEnabled: boolean;
  totalHoursThisWeek: number | null;
}

interface User {
  uuid: string;
  name: string;
  email: string;
  token: string;
  refresh_token?: string;
  projects: Project[];
  current_project_id?: string;
}

interface TimeUpdatePayload {
  time: string;
  projectType: string;
  projectId: string;
  targetName: string | null;
}

function MainWindow() {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);
  const [sessionTime, setSessionTime] = useState("--:--:--");
  const [sessionType, setSessionType] = useState<string>("Project");
  const [activeBreakName, setActiveBreakName] = useState<string | null>(null);
  const [showBreakModal, setShowBreakModal] = useState(false);
  const [isActive, setIsActive] = useState(false);
  const [projectTimes, setProjectTimes] = useState<Record<string, string>>({});
  const [permissionsGranted, setPermissionsGranted] = useState(true);
  const [showPermissions, setShowPermissions] = useState(false);
  const initialized = useRef(false);

  useEffect(() => {
    // ... logic to update times
    const interval = setInterval(updateProjectTimes, 60000); // Update every minute
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    if (user) {
      updateProjectTimes();
      checkPermissionsStatus();
    }
  }, [user, sessionTime]);
  // Update when user loads or global timer ticks (optional, maybe overkill to do on every sec, but good for "active" project)

  async function updateProjectTimes() {
    if (!user) return;
    const times: Record<string, string> = {};

    // Parallel fetch could be better but simple loop is fine for few projects
    for (const p of user.projects) {
      try {
        const t = await invoke<string>("get_project_today_total", { projectId: p.id });
        times[p.id] = t;
      } catch (e) {
        console.error("Failed to fetch time for project", p.name, e);
        times[p.id] = "00:00:00";
      }
    }
    setProjectTimes(times);
  }

  async function checkPermissionsStatus() {
    try {
      const res = await invoke<{ accessibility: boolean; screenRecording: boolean }>("check_permissions");
      console.log(res)
      const granted = res.accessibility && res.screenRecording;
      setPermissionsGranted(granted);

      // If we have a project but no permissions, show the modal
      if (!granted && user?.current_project_id) {
        setShowPermissions(true);
      } else if (granted) {
        setShowPermissions(false);
      }
    } catch (e) {
      console.error("Failed to check permissions status", e);
    }
  }

  const checkForUpdates = async () => {
    try {
      console.log("Checking for updates")
      const update = await check();
      if (update) {
        console.log(`Update to ${update.version} available!`);
        await update.downloadAndInstall();
        // This will restart the app automatically after the install
        await relaunch();
      } else {
        console.log("No update found.");
      }
    } catch (error) {
      console.log(error)
    }
  }

  const totalToday = useMemo(() => {
    let totalSeconds = 0;
    Object.values(projectTimes).forEach(timeStr => {
      const parts = timeStr.split(':').map(Number);
      if (parts.length === 3) {
        totalSeconds += parts[0] * 3600 + parts[1] * 60 + parts[2];
      }
    });
    const h = Math.floor(totalSeconds / 3600);
    const m = Math.floor((totalSeconds % 3600) / 60);
    return `${h}:${m.toString().padStart(2, '0')}`;
  }, [projectTimes]);

  useEffect(() => {
    checkForUpdates();
    // Listeners setup only
    if (!initialized.current) {
      initialized.current = true;
      checkAuth();
    }
    const unlistenLogin = listen("request-login", () => checkAuth());
    const unlistenLogout = listen("logout-user", () => checkAuth());
    const unlistenTime = listen<TimeUpdatePayload>("time-update", (event) => {
      setSessionTime(event.payload.time);
      setSessionType(event.payload.projectType);
      setActiveBreakName(event.payload.targetName);
    });

    const unlistenActive = listen<boolean>("timer-active", (event) => {
      setIsActive(event.payload);
    });

    const unlistenLimit = listen<string>("limit-reached", (event) => {
      alert(event.payload);
    });

    return () => {
      unlistenLogin.then(f => f());
      unlistenLogout.then(f => f());
      unlistenTime.then(f => f());
      unlistenActive.then(f => f());
      unlistenLimit.then(f => f());
    };
  }, []);

  async function checkAuth() {
    try {
      let user = await invoke<User | null>("check_auth");
      if (user && user.token) {
        try {
          // Fetch fresh projects
          console.log(user.token)
          const fetchedProjects = await fetchProjects(user.token);
          const mappedProjects: Project[] = fetchedProjects.map(p => ({
            id: p.id,
            name: p.name,
            weeklyLimitHours: p.weeklyLimitHours,
            dailyLimitHours: p.dailyLimitHours,
            screenshotsEnabled: p.screenshotsEnabled,
            totalHoursThisWeek: p.totalHoursThisWeek
          }));

          // Update user object
          user = { ...user, projects: mappedProjects };

          // Persist valid projects to DB
          await invoke("login", { user });
        } catch (e) {
          console.error("Failed to refresh projects in background", e);
        }
      }
      setUser(user);

      // Fetch initial timer status
      try {
        const active = await invoke<boolean>("get_timer_status");
        setIsActive(active);
      } catch (e) {
        console.error("Failed to fetch timer status", e);
      }
    } catch (err) {
      console.error("Auth check failed", err);
    } finally {
      setLoading(false);
    }
  }

  async function handleLogout() {
    await invoke("logout");
    checkAuth();
  }

  async function handleProjectSelect(projectId: string) {
    if (!user) return;
    try {
      await invoke("set_current_project", { projectId });
      setUser({ ...user, current_project_id: projectId });

      // Re-check permissions when project is selected
      const res = await invoke<{ accessibility: boolean; screenRecording: boolean }>("check_permissions");
      if (!res.accessibility || !res.screenRecording) {
        setShowPermissions(true);
      }
    } catch (err) {
      console.error("Failed to set project", err);
    }
  }

  async function toggleTimer() {
    try {
      if (isActive) {
        await invoke("stop_timer");
      } else {
        // Frontend limit check
        if (currentProject) {
          if (currentProject.dailyLimitHours) {
            // Simple check against today's total from component state or recalculated
            // For better UX, we'll let the backend verify accurately, but we can do a quick check
          }
        }
        await invoke("start_timer");
      }
    } catch (err) {
      console.error("Failed to toggle timer", err);
      if (typeof err === "string" && err.includes("limit reached")) {
        alert(err);
      } else {
        alert("Failed to toggle timer: " + err);
      }
    }
  }

  const sortedProjects = useMemo(() => {
    if (!user || !user.projects) return [];
    return [...user.projects].sort((a, b) => {
      if (a.id === user.current_project_id) return -1;
      if (b.id === user.current_project_id) return 1;
      return 0;
    });
  }, [user]);

  if (loading) return <div className="h-screen flex items-center justify-center">Loading...</div>;

  if (!user) {
    return <Login onLogin={(u) => setUser(u as User)} />;
  }

  const currentProject = user.projects.find(p => p.id === user.current_project_id) || user.projects[0];

  return (
    <div className="flex h-screen bg-[#F8FAFC] text-gray-800 font-sans selection:bg-primary/10 overflow-hidden">
      {showPermissions && <PermissionsModal onGranted={() => {
        setPermissionsGranted(true);
        setShowPermissions(false);
      }} />}

      {showBreakModal && user?.current_project_id && (
        <BreakModal
          projectId={user.current_project_id}
          token={user.token}
          onClose={() => setShowBreakModal(false)}
          onStartBreak={() => setIsActive(true)}
        />
      )}

      {/* Sidebar - Project List */}
      <aside className="w-72 shrink-0 border-r border-gray-200 flex flex-col bg-white shadow-sm z-10">
        <div className="p-5 border-b border-gray-50">
          <div className="flex items-center gap-2.5 mb-6">
            <div className="w-8 h-8 rounded-lg bg-primary flex items-center justify-center text-white shadow-lg shadow-primary/20">
              <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
            </div>
            <div>
              <h1 className="text-sm font-bold text-gray-900 tracking-tight">WATCHTOWER</h1>
              <p className="text-[9px] text-gray-400 font-bold uppercase tracking-widest">Time Track</p>
            </div>
          </div>

          <div className="relative group">
            <input
              type="text"
              placeholder="Search projects..."
              className="w-full pl-9 pr-4 py-2 bg-gray-50 border border-gray-100 rounded-lg text-xs placeholder-gray-400 focus:outline-none focus:bg-white focus:border-primary/30 focus:ring-4 focus:ring-primary/5 transition-all duration-200"
            />
            <svg xmlns="http://www.w3.org/2000/svg" className="h-3.5 w-3.5 absolute left-3 top-2.5 text-gray-400 group-focus-within:text-primary transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto px-3 py-4 custom-scrollbar">
          <div className="flex items-center justify-between px-2 mb-3">
            <span className="text-[10px] font-bold text-gray-400 uppercase tracking-widest">My Projects</span>
            <span className="text-[9px] bg-gray-100 text-gray-500 px-1.5 py-0.5 rounded-full font-bold">{sortedProjects.length}</span>
          </div>

          <div className="space-y-1 pb-4">
            {sortedProjects.map(p => (
              <div
                key={p.id}
                onClick={() => handleProjectSelect(p.id)}
                className={`group relative flex items-center justify-between p-2.5 rounded-xl cursor-pointer transition-all duration-200 border ${user.current_project_id === p.id
                  ? 'bg-white border-primary shadow-sm ring-2 ring-primary/5'
                  : 'bg-transparent border-transparent hover:bg-gray-50 hover:border-gray-100'}`}
              >
                <div className="flex items-center gap-2.5 min-w-0">
                  <div className={`w-1.5 h-1.5 rounded-full shrink-0 ${user.current_project_id === p.id ? 'bg-primary animate-pulse' : 'bg-gray-300'}`} />
                  <span className={`truncate text-xs font-semibold transition-colors ${user.current_project_id === p.id ? 'text-gray-900' : 'text-gray-600 group-hover:text-gray-900'}`}>{p.name}</span>
                </div>
                <div className="text-[10px] font-bold text-gray-400 font-mono tracking-tighter">
                  {projectTimes[p.id]?.split(':').slice(0, 2).join(':') || '0:00'}
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* User Profile */}
        <div className="p-4 mx-4 mb-6 bg-gray-50 rounded-2xl border border-gray-100">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="relative">
                <div className="w-10 h-10 rounded-xl bg-linear-to-br from-primary to-secondary flex items-center justify-center text-white text-sm font-bold shadow-md">
                  {user.name.charAt(0).toUpperCase()}
                </div>
                <div className="absolute -right-1 -bottom-1 w-3.5 h-3.5 bg-green-500 border-2 border-white rounded-full" title="Online" />
              </div>
              <div className="flex flex-col">
                <span className="text-sm font-bold text-gray-900 leading-none">{user.name}</span>
                <span className="text-[10px] text-gray-500 font-medium mt-1">Workspace Admin</span>
              </div>
            </div>
            <button
              onClick={handleLogout}
              className="p-2 text-gray-400 hover:text-red-500 hover:bg-red-50 transition-all rounded-lg cursor-pointer"
              title="Logout"
            >
              <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
              </svg>
            </button>
          </div>
        </div>
      </aside>

      {/* Main Content Area */}
      <main className="flex-1 flex flex-col min-w-0 bg-[#F8FAFC] relative overflow-hidden">
        {/* Decorative elements */}
        <div className="absolute top-0 right-0 w-96 h-96 bg-primary/5 rounded-full blur-3xl -mr-48 -mt-48" />
        <div className="absolute bottom-0 left-0 w-64 h-64 bg-secondary/5 rounded-full blur-3xl -ml-32 -mb-32" />


        {/* Dashboard Content */}
        <div className="flex-1 overflow-y-auto p-4 z-10 custom-scrollbar bg-gray-50/30">
          <div className="max-w-7xl mx-auto space-y-4">

            {/* Ultra-Dense Session Bar */}
            <div className="bg-white p-1.5 rounded-xl border border-gray-200 shadow-xs flex items-center justify-between gap-4">
              <div className="flex items-center gap-3 pl-2">
                <div className={`w-2 h-2 rounded-full ${isActive ? 'bg-primary animate-pulse' : 'bg-gray-300'}`} />
                <div className="min-w-0">
                  <span className="text-[9px] font-bold text-gray-400 uppercase tracking-tighter leading-none block mb-0.5">Focusing on</span>
                  <h3 className="text-xs font-bold text-gray-900 truncate max-w-[180px]" title={sessionType === "WorkBreakPolicy" ? (activeBreakName || "Break") : (currentProject?.name || "Select Project")}>
                    {sessionType === "WorkBreakPolicy" ? (activeBreakName || "Break") : (currentProject?.name || "Select Project")}
                  </h3>
                </div>
              </div>

              <div className="flex-1 flex items-center justify-center gap-6">
                <div className="flex items-baseline gap-1.5">
                  <span className="text-xl font-mono font-bold text-gray-900 tabular-nums tracking-tighter">
                    {sessionTime === "--:--:--" ? "00:00:00" : sessionTime}
                  </span>
                  <span className="text-[10px] font-bold text-primary uppercase">Duration</span>
                </div>
                <div className="h-6 w-px bg-gray-100" />
                <div className="flex items-center gap-4">
                  <div className="text-center">
                    <span className="text-[9px] font-bold text-gray-400 uppercase leading-none block">Today</span>
                    <span className="text-[11px] font-bold text-gray-700">{projectTimes[currentProject?.id || '']?.split(':').slice(0, 2).join(':') || '00:00'}</span>
                  </div>
                  <div className="text-center">
                    <span className="text-[9px] font-bold text-gray-400 uppercase leading-none block">Weekly</span>
                    <span className="text-[11px] font-bold text-gray-700">{((currentProject?.totalHoursThisWeek || 0)).toFixed(1)}h</span>
                  </div>
                </div>
              </div>

              <div className="flex items-center gap-1.5 pr-1">
                {isActive && sessionType === "Project" && (
                  <button
                    onClick={() => setShowBreakModal(true)}
                    className="h-8 px-3 rounded-lg text-[10px] font-bold text-orange-600 bg-orange-50 hover:bg-orange-100 transition-all cursor-pointer border border-orange-100"
                  >
                    BREAK
                  </button>
                )}
                <button
                  onClick={toggleTimer}
                  disabled={!permissionsGranted && !isActive}
                  className={`h-8 px-5 rounded-lg flex items-center justify-center gap-2 transition-all font-bold text-[10px] cursor-pointer ${isActive
                    ? 'bg-red-500 hover:bg-red-600 text-white'
                    : !permissionsGranted
                      ? 'bg-gray-100 text-gray-300 cursor-not-allowed'
                      : 'bg-primary hover:opacity-95 text-white'
                    }`}
                >
                  {isActive ? 'STOP' : 'START SESSION'}
                </button>
              </div>
            </div>

            <div className="grid grid-cols-12 gap-4">
              {/* Main Content Area */}
              <div className="col-span-12 lg:col-span-8">
                <div className="bg-white rounded-xl border border-gray-100 shadow-xs overflow-hidden">
                  <div className="px-4 py-3 border-b border-gray-50 flex items-center justify-between">
                    <h3 className="text-[11px] font-bold text-gray-500 uppercase tracking-widest">Recent Activity</h3>
                  </div>
                  <div className="p-1">
                    {user.projects.slice(0, 6).map((p, idx) => (
                      <div key={p.id} className="flex items-center justify-between px-3 py-2.5 rounded-lg hover:bg-gray-50 transition-all group">
                        <div className="flex items-center gap-3">
                          <div className={`w-1.5 h-1.5 rounded-full ${idx === 0 ? 'bg-primary' : 'bg-gray-200'}`} />
                          <span className="text-xs font-semibold text-gray-700">{p.name}</span>
                        </div>
                        <div className="flex items-center gap-6">
                          <span className="text-[11px] font-mono font-medium text-gray-400">2h ago</span>
                          <span className="text-xs font-bold text-gray-900 w-16 text-right">{projectTimes[p.id]?.split(':').slice(0, 2).join(':') || "00:00"}</span>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              </div>

              {/* Sidebar Stats Area */}
              <div className="col-span-12 lg:col-span-4 space-y-3">
                <div className="bg-white p-4 rounded-xl border border-gray-100 shadow-xs">
                  <h4 className="text-[10px] font-bold text-gray-400 uppercase mb-3">Today's Total</h4>
                  <div className="space-y-3">
                    <div className="flex justify-between items-baseline">
                      <span className="text-2xl font-mono font-bold text-gray-900">{totalToday}h</span>
                      <span className="text-[10px] font-bold text-green-500 bg-green-50 px-1.5 py-0.5 rounded-md">ON TRACK</span>
                    </div>
                    <div className="h-1.5 w-full bg-gray-50 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-primary transition-all duration-1000"
                        style={{ width: `${Math.min((parseFloat(totalToday.replace(':', '.')) / 8) * 100, 100)}%` }}
                      />
                    </div>
                    <p className="text-[9px] text-gray-400 font-medium">Daily goal: 8.0 hours</p>
                  </div>
                </div>

                <div className="bg-white p-4 rounded-xl border border-gray-100 shadow-xs flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <div className="w-1.5 h-1.5 rounded-full bg-green-500" />
                    <span className="text-[10px] font-bold text-gray-400 uppercase">System Status</span>
                  </div>
                  <span className="text-[10px] font-bold text-gray-900 uppercase">Online</span>
                </div>
              </div>
            </div>
          </div>
        </div>



      </main>
    </div>
  );
}

function App() {
  const [version, setVersion] = useState("");

  useEffect(() => {
    getVersion().then(setVersion).catch(console.error);
  }, []);

  return (
    <>
      <Routes>
        <Route path="/" element={<MainWindow />} />
        <Route path="/idle" element={<IdleWindow />} />
        <Route path="/quit" element={<QuitWindow />} />
      </Routes>
      {version && (
        <div className="fixed bottom-1 right-2 text-[10px] text-gray-400 font-mono tracking-wider z-50 opacity-70 pointer-events-none">
          Version: v{version}
        </div>
      )}
    </>
  );
}

export default App;
