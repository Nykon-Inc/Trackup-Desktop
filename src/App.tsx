import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Routes, Route } from "react-router-dom";
import Login from "./Login";
import "./App.css";
import { IdleWindow } from "./components/IdleWindow";
import { QuitWindow } from "./components/QuitWindow";

interface Project {
  id: string;
  name: string;
}

interface User {
  uuid: string;
  name: string;
  email: string;
  token: string;
  projects: Project[];
  current_project_id?: string;
}

function MainWindow() {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);
  const [sessionTime, setSessionTime] = useState("--:--:--");
  const [isActive, setIsActive] = useState(false);

  useEffect(() => {
    checkAuth();

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
      const user = await invoke<User | null>("check_auth");
      setUser(user);
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

  if (loading) return <div className="h-screen flex items-center justify-center">Loading...</div>;

  if (!user) {
    return <Login onLogin={(u) => setUser(u as User)} />;
  }

  const currentProject = user.projects.find(p => p.id === user.current_project_id) || user.projects[0];

  return (
    <div className="flex h-screen bg-white text-gray-800 font-sans selection:bg-blue-100">
      {/* Sidebar */}
      <div className="w-80 shrink-0 border-r border-gray-200 flex flex-col bg-white">
        {/* Timer Section */}
        <div className="p-6 flex flex-col items-center border-b border-gray-100">
          <div className="bg-gray-800 text-white px-6 py-2 rounded-full text-3xl font-mono font-bold mb-4 tracking-wider shadow-sm">
            {sessionTime === "--:--:--" ? "00:00:00" : sessionTime}
          </div>

          <h2 className="text-lg font-bold text-gray-900 mb-6 truncate max-w-full" title={currentProject?.name}>
            {currentProject?.name || "Select Project"}
          </h2>

          <button
            onClick={toggleTimer}
            className={`w-16 h-16 rounded-full flex items-center justify-center transition-all shadow-lg hover:shadow-xl active:scale-95 cursor-pointer ${isActive ? 'bg-red-500 hover:bg-red-600' : 'bg-blue-500 hover:bg-blue-600'}`}
          >
            {isActive ? (
              // Stop Icon
              <svg xmlns="http://www.w3.org/2000/svg" className="h-8 w-8 text-white" viewBox="0 0 24 24" fill="currentColor">
                <rect x="6" y="6" width="12" height="12" rx="1" />
              </svg>
            ) : (
              // Play Icon
              <svg xmlns="http://www.w3.org/2000/svg" className="h-8 w-8 text-white ml-1" viewBox="0 0 24 24" fill="currentColor">
                <path d="M8 5v14l11-7z" />
              </svg>
            )}
          </button>

          <div className="flex items-center justify-between w-full mt-8 text-xs text-gray-400 font-medium px-2">
            <span>No limits</span>
            <span>Today: {sessionTime === "--:--:--" ? "0:00" : sessionTime.split(':').slice(0, 2).join(':')}</span>
          </div>
        </div>

        {/* Project List Section */}
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="p-4">
            <div className="relative">
              <input
                type="text"
                placeholder="Search projects"
                className="w-full pl-9 pr-4 py-2 bg-gray-50 border border-transparent rounded-lg text-sm text-gray-700 placeholder-gray-400 focus:outline-none focus:bg-white focus:border-blue-500 focus:ring-2 focus:ring-blue-100 transition-all"
              />
              <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4 absolute left-3 top-2.5 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
              </svg>
            </div>
          </div>

          <div className="overflow-y-auto flex-1 px-2 pb-4">
            <div className="px-3 py-2 text-xs font-bold text-gray-400 uppercase tracking-wider">
              My Projects
            </div>
            <div className="space-y-0.5">
              {user.projects.map(p => (
                <div
                  key={p.id}
                  onClick={() => handleProjectSelect(p.id)}
                  className={`group flex items-center justify-between px-3 py-2 rounded-md cursor-pointer transition-colors ${user.current_project_id === p.id ? 'bg-blue-50 text-blue-700' : 'text-gray-600 hover:bg-gray-50'}`}
                >
                  <div className="flex items-center gap-2 overflow-hidden">
                    {user.current_project_id === p.id ? (
                      <div className="w-1.5 h-1.5 rounded-full bg-blue-500 shrink-0" />
                    ) : (
                      <div className="w-1.5 h-1.5 opacity-0 group-hover:opacity-100 rounded-full bg-gray-300 shrink-0 transition-opacity" />
                    )}
                    <span className="truncate text-sm font-medium">{p.name}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* User Profile / Logout */}
          <div className="p-4 border-t border-gray-100">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <div className="w-8 h-8 rounded-full bg-linear-to-br from-indigo-500 to-purple-500 flex items-center justify-center text-white text-xs font-bold shadow-xs">
                  {user.name.charAt(0).toUpperCase()}
                </div>
                <div className="flex flex-col">
                  <span className="text-sm font-semibold text-gray-700 leading-tight">{user.name}</span>
                  <span className="text-sm text-gray-400 leading-tight">Free Plan</span>
                </div>
              </div>
              <button
                onClick={handleLogout}
                className="p-1.5 text-gray-400 hover:text-red-500 hover:bg-red-50 rounded-md transition-colors cursor-pointer"
                title="Logout"
              >
                <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
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
