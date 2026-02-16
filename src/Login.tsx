import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import LoginLayout from "./layouts/LoginLayout";
import { fadeInBackgroundElements } from "./utils/layoutFunctions";
import { authenticateUser } from "./services/auth";

export default function Login({ onLogin }: { onLogin: (user: any) => void }) {
    const [email, setEmail] = useState("");
    const [password, setPassword] = useState("");
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        fadeInBackgroundElements();
    }, []);

    async function handleLogin(e: React.FormEvent) {
        e.preventDefault();
        if (!email || !password) return;

        setIsLoading(true);
        setError(null);

        try {
            const userPayload = await authenticateUser(email, password);
            console.log(userPayload)
            await invoke("login", { user: userPayload });
            onLogin(userPayload);
        } catch (err) {
            console.error("Login failed", err);
            const message = err instanceof Error ? err.message : "Invalid email or password";
            setError(message);
        } finally {
            setIsLoading(false);
        }
    }

    return (
        <LoginLayout>
            <div className="login-container relative z-10 bg-white/95 backdrop-blur-md p-7 rounded-2xl shadow-xl w-full max-w-[340px] border border-white/20 animate-in fade-in zoom-in duration-500 mt-[-20px]">
                <div className="flex flex-col items-center mb-6">
                    <div className="w-12 h-12 bg-primary rounded-xl flex items-center justify-center shadow-lg shadow-black/10 mb-3 transform -rotate-3 text-white">
                        <svg className="w-7 h-7" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2.5" d="M13 10V3L4 14h7v7l9-11h-7z" />
                        </svg>
                    </div>
                    <h2 className="text-xl font-bold text-gray-900">Welcome to Trackup</h2>
                    <p className="text-xs text-gray-500 mt-1">Sign in to your account</p>
                </div>

                <form className="login-form flex flex-col gap-3.5" onSubmit={handleLogin}>
                    {error && (
                        <div className="bg-red-50 border border-red-100 text-red-600 px-3 py-2 rounded-lg text-xs animate-shake">
                            {error}
                        </div>
                    )}

                    <div className="space-y-1">
                        <label className="text-[11px] uppercase tracking-wider font-semibold text-gray-400 ml-1">Email</label>
                        <input
                            type="email"
                            placeholder="your@email.com"
                            value={email}
                            disabled={isLoading}
                            onChange={(e) => setEmail(e.target.value)}
                            className="w-full px-3.5 py-2.5 rounded-lg bg-gray-50 border border-gray-100 text-sm text-gray-800 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-primary/10 focus:border-primary transition-all disabled:opacity-50"
                        />
                    </div>

                    <div className="space-y-1">
                        <label className="text-[11px] uppercase tracking-wider font-semibold text-gray-400 ml-1">Password</label>
                        <input
                            type="password"
                            placeholder="••••••••"
                            value={password}
                            disabled={isLoading}
                            onChange={(e) => setPassword(e.target.value)}
                            className="w-full px-3.5 py-2.5 rounded-lg bg-gray-50 border border-gray-100 text-sm text-gray-800 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-primary/10 focus:border-primary transition-all disabled:opacity-50"
                        />
                    </div>

                    <button
                        type="submit"
                        disabled={isLoading || !email || !password}
                        className="w-full py-3 px-6 rounded-lg bg-primary text-white text-sm font-semibold hover:opacity-90 focus:outline-none focus:ring-4 focus:ring-primary/20 transform transition-all active:scale-[0.97] shadow-lg shadow-black/5 mt-3 cursor-pointer disabled:opacity-70 disabled:cursor-not-allowed flex items-center justify-center gap-2"
                    >
                        {isLoading ? (
                            <>
                                <svg className="animate-spin h-4 w-4 text-white" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                                </svg>
                                <span>Signing in...</span>
                            </>
                        ) : (
                            "Sign In"
                        )}
                    </button>
                </form>

                <div className="mt-6 text-center text-[10px] text-gray-400 tracking-widest uppercase">
                    Trackup Desktop 2.0
                </div>
            </div>
        </LoginLayout>
    );
}
