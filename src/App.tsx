import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Login from "./Login";
import "./App.css";

interface Project {
  id: string;
  name: string;
  role: string;
}

interface User {
  uuid: string;
  name: string;
  email: string;
  role: string;
  token: string;
  projects: Project[];
  current_project_id?: string;
}

function App() {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    checkAuth();

    // Listen for tray menu login request
    const unlisten = listen("request-login", () => {
      checkAuth();
    });

    return () => {
      unlisten.then(f => f());
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
    setUser(null);
  }

  async function handleProjectChange(e: React.ChangeEvent<HTMLSelectElement>) {
    const projectId = e.target.value;
    if (!user) return;

    try {
      await invoke("set_current_project", { projectId });
      setUser({ ...user, current_project_id: projectId });
    } catch (err) {
      console.error("Failed to set project", err);
    }
  }

  if (loading) return <div>Loading...</div>;

  return (
    <>
      {user ? (
        <div>
          <h1>Welcome, {user.name}</h1>
          <p className="subtitle" style={{ opacity: 0.7, marginBottom: "2rem" }}>
            Logged in as {user.email} <span style={{ background: "#333", padding: "2px 6px", borderRadius: "4px", fontSize: "0.8em" }}>{user.role}</span>
          </p>

          <div className="project-selector" style={{ margin: "2rem 0", display: "flex", alignItems: "center", justifyContent: "center", gap: "1rem" }}>
            <label>Current Project:</label>
            <select
              value={user.current_project_id || ""}
              onChange={handleProjectChange}
              style={{ padding: "8px 12px", borderRadius: "8px", border: "1px solid #ccc", minWidth: "200px" }}
            >
              <option value="" disabled>Select a Project</option>
              {user.projects.map(p => (
                <option key={p.id} value={p.id}>
                  {p.name} ({p.role})
                </option>
              ))}
            </select>
          </div>

          <div className="row">
            <button onClick={handleLogout} style={{ background: "#d32f2f", color: "white", borderColor: "#b71c1c" }}>Logout</button>
          </div>
        </div>
      ) : (
        <Login onLogin={(u) => setUser(u as User)} />
      )}
    </>
  );
}

export default App;
