import { useState, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Routes, Route } from "react-router-dom";
import Login from "./Login";
import { fetchProjects } from "./services/auth";
import "./App.css";
import { IdleWindow } from "./components/IdleWindow";
import { QuitWindow } from "./components/QuitWindow";
import { PermissionsModal } from "./components/PermissionsModal";

interface Project {
  id: string;
  name: string;
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

function MainWindow() {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);
  const [sessionTime, setSessionTime] = useState("--:--:--");
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
    // Listeners setup only
    if (!initialized.current) {
      initialized.current = true;
      checkAuth();
    }
    const unlistenLogin = listen("request-login", () => checkAuth());
    const unlistenLogout = listen("logout-user", () => checkAuth());
    const unlistenTime = listen<string>("time-update", (event) => {
      setSessionTime(event.payload);
    });

    const unlistenActive = listen<boolean>("timer-active", (event) => {
      setIsActive(event.payload);
    });

    return () => {
      unlistenLogin.then(f => f());
      unlistenLogout.then(f => f());
      unlistenTime.then(f => f());
      unlistenActive.then(f => f());
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
          const mappedProjects = fetchedProjects.map(p => ({ id: p.id, name: p.name }));

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
        await invoke("start_timer");
      }
    } catch (err) {
      console.error("Failed to toggle timer", err);
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
    <div className="flex h-screen bg-white text-gray-800 font-sans selection:bg-primary/10">
      {showPermissions && <PermissionsModal onGranted={() => {
        setPermissionsGranted(true);
        setShowPermissions(false);
      }} />}

      {/* Sidebar */}
      <div className="w-72 shrink-0 border-r border-gray-200 flex flex-col bg-white">
        {/* Timer Section */}
        <div className="p-5 flex flex-col items-center border-b border-gray-100">
          <div className="bg-primary text-white px-5 py-1.5 rounded-full text-2xl font-mono font-bold mb-4 tracking-wider shadow-sm">
            {sessionTime === "--:--:--" ? "00:00:00" : sessionTime}
          </div>

          <h2 className="text-base font-bold text-gray-900 mb-5 truncate max-w-full" title={currentProject?.name}>
            {currentProject?.name || "Select Project"}
          </h2>

          <button
            onClick={toggleTimer}
            disabled={!permissionsGranted && !isActive}
            className={`w-12 h-12 rounded-full flex items-center justify-center transition-all shadow-md hover:shadow-lg active:scale-95 cursor-pointer ${(isActive) ? 'bg-red-500 hover:bg-red-600' : (!permissionsGranted ? 'bg-gray-300 cursor-not-allowed text-gray-400 shadow-none' : 'bg-primary hover:opacity-90')}`}
          >
            {isActive ? (
              // Stop Icon
              <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6 text-white" viewBox="0 0 24 24" fill="currentColor">
                <rect x="7" y="7" width="10" height="10" rx="1" />
              </svg>
            ) : (
              // Play Icon
              <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6 text-white ml-1" viewBox="0 0 24 24" fill="currentColor">
                <path d="M8 5v14l11-7z" />
              </svg>
            )}
          </button>

          <div className="flex items-center justify-between w-full mt-6 text-[11px] text-gray-400 font-semibold px-2">
            <span>NO LIMITS</span>
            <span>TODAY: {totalToday}</span>
          </div>
        </div>

        {/* Project List Section */}
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="p-3">
            <div className="relative">
              <input
                type="text"
                placeholder="Search projects"
                className="w-full pl-8 pr-3 py-1.5 bg-gray-50 border border-transparent rounded-lg text-[13px] text-gray-700 placeholder-gray-400 focus:outline-none focus:bg-white focus:border-primary focus:ring-2 focus:ring-primary/10 transition-all"
              />
              <svg xmlns="http://www.w3.org/2000/svg" className="h-3.5 w-3.5 absolute left-2.5 top-2.5 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
              </svg>
            </div>
          </div>

          <div className="overflow-y-auto flex-1 px-1.5 pb-3">
            <div className="px-3 py-1.5 text-[10px] font-bold text-gray-400 uppercase tracking-widest">
              My Projects
            </div>


            <div className="space-y-0.5">
              {sortedProjects.map(p => (
                <div
                  key={p.id}
                  onClick={() => handleProjectSelect(p.id)}
                  className={`group flex items-center justify-between px-3 py-1.5 rounded-lg cursor-pointer transition-colors ${user.current_project_id === p.id ? 'bg-primary/5 text-primary' : 'text-gray-600 hover:bg-gray-50'}`}
                >
                  <div className="flex items-center gap-2 overflow-hidden">
                    {user.current_project_id === p.id ? (
                      <div className="w-1 h-1 rounded-full bg-primary shrink-0" />
                    ) : (
                      <div className="w-1 h-1 opacity-0 group-hover:opacity-100 rounded-full bg-gray-300 shrink-0 transition-opacity" />
                    )}
                    <span className="truncate text-[13px] font-medium">{p.name}</span>
                  </div>
                  <div className="text-[11px] text-gray-400 font-mono ml-2">
                    {projectTimes[p.id]?.split(':').slice(0, 2).join(':') || '0:00'}
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* User Profile / Logout */}
          <div className="p-3 border-t border-gray-100 bg-gray-50/30">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <div className="w-7 h-7 rounded-lg bg-primary flex items-center justify-center text-white text-[11px] font-bold shadow-sm">
                  {user.name.charAt(0).toUpperCase()}
                </div>
                <div className="flex flex-col">
                  <span className="text-[13px] font-bold text-gray-700 leading-none">{user.name}</span>
                  <span className="text-[10px] text-gray-400 font-medium mt-0.5">Free Plan</span>
                </div>
              </div>
              <button
                onClick={handleLogout}
                className="p-1.5 text-gray-400 hover:text-red-500 transition-colors cursor-pointer"
                title="Logout"
              >
                <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
                </svg>
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="flex-1 flex flex-col bg-gray-50/50">

      </div>
    </div>
  );
}

function App() {
  return (
    <Routes>
      <Route path="/" element={<MainWindow />} />
      <Route path="/idle" element={<IdleWindow />} />
      <Route path="/quit" element={<QuitWindow />} />
    </Routes>
  );
}

export default App;
