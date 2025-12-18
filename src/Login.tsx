import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import LoginLayout from "./layouts/LoginLayout";
import { fadeInBackgroundElements } from "./utils/layoutFunctions";

export default function Login({ onLogin }: { onLogin: (user: any) => void }) {
    const [email, setEmail] = useState("");


    const [password, setPassword] = useState("");

    useEffect(() => {
        fadeInBackgroundElements();
    }, []);

    async function handleLogin(e: React.FormEvent) {
        e.preventDefault();
        if (!email) return;

        // Simulate getting a full user object from a remote API
        const user = {
            uuid: "user-uuid-" + Date.now(),
            name: email.split('@')[0],
            email: email,
            role: "admin",
            token: "dummy-token-" + Date.now(),
            current_project_id: null,
            projects: [
                { id: "proj-1", name: "Alpha Protocol", role: "lead" },
                { id: "proj-2", name: "Beta Tester", role: "viewer" }
            ]
        };

        try {
            await invoke("login", { user });
            onLogin(user);
        } catch (err) {
            console.error("Login failed", err);
            alert("Login failed: " + err);
        }
    }

    return (
        <LoginLayout>
            <div className="login-container relative z-10 bg-white p-8 rounded-2xl shadow-xl w-full max-w-md border border-gray-100">
                <h2 className="text-2xl font-semibold text-gray-800 mb-6 text-center">Please Log In</h2>
                <form className="login-form flex flex-col gap-4" onSubmit={handleLogin}>
                    <input
                        type="email"
                        placeholder="Enter your email"
                        value={email}
                        onChange={(e) => setEmail(e.target.value)}
                        className="w-full px-4 py-3 rounded-lg bg-gray-50 border border-gray-200 text-gray-800 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all"
                    />
                    <input
                        type="password"
                        placeholder="Enter your password"
                        value={password}
                        onChange={(e) => setPassword(e.target.value)}
                        className="w-full px-4 py-3 rounded-lg bg-gray-50 border border-gray-200 text-gray-800 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all"
                    />
                    <button
                        type="submit"
                        className="w-full py-3 px-6 rounded-lg bg-linear-to-r from-blue-500 to-blue-600 text-white font-medium hover:from-blue-600 hover:to-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-400 transform transition-all active:scale-95 shadow-lg mt-2 cursor-pointer"
                    >
                        Login
                    </button>
                </form>
            </div>
        </LoginLayout>
    );
}
