import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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

export function IdleWindow() {
    const [idleTime, setIdleTime] = useState<number | null>(null);
    const [user, setUser] = useState<User | null>(null);
    const [keepChoice, setKeepChoice] = useState<boolean>(false); // Default: Discard (No)

    useEffect(() => {
        const checkIdle = () => {
            invoke<number | null>("get_idle_time").then((res) => {
                if (res !== null) {
                    setIdleTime(res);
                }
            });
        };

        const fetchUser = async () => {
            try {
                const u = await invoke<User | null>("check_auth");
                setUser(u);
            } catch (err) {
                console.error("Failed to fetch user", err);
            }
        };

        checkIdle();
        fetchUser();

        const unlistenFocus = listen("tauri://focus", () => {
            checkIdle();
        });

        const unlistenIdle = listen<number>("idle_ended", (event) => {
            setIdleTime(event.payload);
        });

        return () => {
            unlistenFocus.then((f) => f());
            unlistenIdle.then((f) => f());
        };
    }, []);

    async function handleChoice(resume: boolean) {
        if (idleTime === null) return;
        try {
            await invoke("process_idle_choice", {
                idleTime,
                keep: keepChoice,
                resume
            });
        } catch (err) {
            console.error("Failed to process idle choice", err);
        }
    }

    if (idleTime === null) {
        return (
            <div className="h-screen bg-gray-100 flex items-center justify-center font-sans">
                <div className="text-gray-500 font-medium">Waiting for idle status...</div>
            </div>
        );
    }

    const minutes = Math.floor(idleTime / 60);
    const currentProject = user?.projects.find(p => p.id === user.current_project_id);

    return (
        <div className="h-screen flex items-center justify-center font-sans select-none overflow-hidden">
            <div className="w-full bg-white rounded-3xl shadow-[0_10px_40px_rgba(0,0,0,0.08)] p-8 flex flex-col">
                {/* Info Card */}
                <div className="border border-[#E8EAED] rounded-2xl p-6 mb-8">
                    <div className="mb-4">
                        <span className="text-[10px] font-bold text-[#9AA0A6] tracking-wider uppercase">
                            YOU HAVE BEEN IDLE FOR
                        </span>
                        <div className="text-lg font-bold text-[#202124] mt-1">
                            {minutes} minutes
                        </div>
                    </div>

                    <div className="h-px bg-[#E8EAED] w-full mb-4"></div>

                    <div className="flex justify-between items-end">
                        <div className="space-y-1">
                            <div className="text-[12px] text-[#5F6368]">
                                <span className="font-medium">Project:</span> {currentProject?.name || "-"}
                            </div>
                        </div>
                    </div>
                </div>

                {/* Choice Section */}
                <div className="mb-10 text-[13px] mt-8">
                    <p className="text-[#202124] font-medium mb-4">Were you working?</p>
                    <div className="space-y-2">
                        <label className="flex items-center gap-3 cursor-pointer group">
                            <div className="relative flex items-center justify-center">
                                <input
                                    type="radio"
                                    name="idleChoice"
                                    checked={!keepChoice}
                                    onChange={() => setKeepChoice(false)}
                                    className="appearance-none w-5 h-5 border-2 border-[#DADCE0] rounded-full checked:border-[#1A73E8] transition-all cursor-pointer"
                                />
                                {!keepChoice && (
                                    <div className="absolute w-2.5 h-2.5 bg-[#1A73E8] rounded-full"></div>
                                )}
                            </div>
                            <span className="text-[#3C4043] font-medium">No, discard idle time</span>
                        </label>

                        <label className="flex items-center gap-3 cursor-pointer group">
                            <div className="relative flex items-center justify-center">
                                <input
                                    type="radio"
                                    name="idleChoice"
                                    checked={keepChoice}
                                    onChange={() => setKeepChoice(true)}
                                    className="appearance-none w-5 h-5 border-2 border-[#DADCE0] rounded-full checked:border-[#1A73E8] transition-all cursor-pointer"
                                />
                                {keepChoice && (
                                    <div className="absolute w-2.5 h-2.5 bg-[#1A73E8] rounded-full"></div>
                                )}
                            </div>
                            <span className="text-[#3C4043] font-medium">Yes, keep idle time</span>
                        </label>
                    </div>
                </div>

                {/* Footer Buttons */}
                <div className="flex items-center justify-end gap-3 mt-auto">
                    <button
                        onClick={() => handleChoice(false)}
                        className="px-4 py-2 text-[11px] font-bold text-[#3C4043] bg-white border border-[#DADCE0] rounded-lg hover:bg-[#F8F9FA] transition-colors cursor-pointer"
                    >
                        Stop timer
                    </button>
                    <button
                        onClick={() => handleChoice(true)}
                        className="px-4 py-2 text-[11px] font-bold text-white bg-[#1A73E8] rounded-lg hover:bg-[#185ABC] shadow-md transition-colors cursor-pointer"
                    >
                        Resume timer
                    </button>
                </div>
            </div>
        </div>
    );
}

