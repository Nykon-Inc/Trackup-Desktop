import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function IdleWindow() {
    const [idleTime, setIdleTime] = useState<number | null>(null);

    useEffect(() => {
        const checkIdle = () => {
            invoke<number | null>("get_idle_time").then((res) => {
                if (res !== null) {
                    console.log("IdleWindow: Fetched idle time:", res);
                    setIdleTime(res);
                }
            });
        };

        // 1. Initial Fetch
        checkIdle();

        // 2. Listen for focus (Reliable trigger when Rust shows the window)
        const unlistenFocus = listen("tauri://focus", () => {
            console.log("IdleWindow: Window focused, re-checking idle time");
            checkIdle();
        });

        // 3. Event Listener (fallback)
        const unlistenIdle = listen<number>("idle_ended", (event) => {
            console.log("IdleWindow received idle_ended event:", event.payload);
            setIdleTime(event.payload);
        });

        return () => {
            unlistenFocus.then((f) => f());
            unlistenIdle.then((f) => f());
        };
    }, []);

    async function handleChoice(keep: boolean) {
        if (idleTime === null) return;
        try {
            await invoke("process_idle_choice", { idleTime, keep });
            // Close this window after choice
            const win = getCurrentWindow();
            await win.hide(); // Hide instead of close to reuse? Or close.
            // If we reuse, hide.
        } catch (err) {
            console.error("Failed to process idle choice", err);
        }
    }

    if (idleTime === null) return <div className="p-4 text-center">Waiting for idle status...</div>;

    const minutes = Math.floor(idleTime / 60);
    const seconds = idleTime % 60;

    return (
        <div className="h-screen bg-gray-50 flex flex-col items-center justify-center p-6">
            <div className="flex items-start gap-4 mb-6">
                <div className="p-3 bg-yellow-100 rounded-full shrink-0">
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6 text-yellow-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                    </svg>
                </div>
                <div className="flex-1">
                    <h3 className="text-lg font-bold text-gray-900">You were away</h3>
                    <p className="mt-2 text-gray-600 leading-relaxed">
                        We detected no activity for <span className="font-bold text-gray-900">{minutes} minutes and {seconds} seconds</span>.
                        Would you like to include this time in your session?
                    </p>
                </div>
            </div>

            <div className="flex gap-3 justify-center w-full">
                <button
                    onClick={() => handleChoice(false)}
                    className="flex-1 max-w-[140px] px-4 py-2.5 text-sm font-semibold text-gray-700 bg-white border border-gray-300 hover:bg-gray-50 rounded-lg transition-colors"
                >
                    Discard Time
                </button>
                <button
                    onClick={() => handleChoice(true)}
                    className="flex-1 max-w-[140px] px-4 py-2.5 text-sm font-semibold text-white bg-blue-600 hover:bg-blue-700 rounded-lg shadow-sm transition-colors"
                >
                    Keep Time
                </button>
            </div>
        </div>
    );
}
