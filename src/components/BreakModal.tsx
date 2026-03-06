import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { fetchWorkBreakPolicies, WorkBreakPolicy } from "../services/auth";

interface BreakModalProps {
    projectId: string;
    token: string;
    onClose: () => void;
    onStartBreak: () => void;
}

export function BreakModal({ projectId, token, onClose, onStartBreak }: BreakModalProps) {
    const [policies, setPolicies] = useState<WorkBreakPolicy[]>([]);
    const [loading, setLoading] = useState(true);
    const [selectedPolicyId, setSelectedPolicyId] = useState<string>("");
    const [starting, setStarting] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        async function loadPolicies() {
            try {
                const [data, usedIds] = await Promise.all([
                    fetchWorkBreakPolicies(token, projectId),
                    invoke<string[]>("get_used_break_ids")
                ]);

                const available = (data || []).filter(p => !usedIds.includes(p.id));
                setPolicies(available);

                if (available.length > 0) {
                    setSelectedPolicyId(available[0].id);
                }
            } catch (err) {
                console.error("Failed to fetch break policies", err);
                setError("Failed to load break policies");
            } finally {
                setLoading(false);
            }
        }
        loadPolicies();
    }, [projectId, token]);

    const handleStartBreak = async () => {
        const selectedPolicy = policies.find(p => p.id === selectedPolicyId);
        if (!selectedPolicy) return;

        setStarting(true);
        try {
            await invoke("start_break", {
                policyId: selectedPolicyId,
                durationMinutes: selectedPolicy.duration,
                targetName: selectedPolicy.name
            });
            onStartBreak();
            onClose();
        } catch (err) {
            console.error("Failed to start break", err);
            setError("Failed to start break session");
            setStarting(false);
        }
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
            {/* Backdrop */}
            <div className="absolute inset-0 bg-gray-900/40 backdrop-blur-sm" onClick={onClose} />

            {/* Modal Content */}
            <div className="relative bg-white rounded-xl shadow-2xl w-full max-w-[320px] overflow-hidden border border-gray-100 animate-in fade-in zoom-in duration-200">
                <div className="p-5">
                    <div className="flex items-center gap-3 mb-4">
                        <div className="w-10 h-10 bg-orange-100 rounded-lg flex items-center justify-center shrink-0">
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5 text-orange-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                        </div>
                        <div>
                            <h2 className="text-lg font-bold text-gray-900 leading-tight">Take a Break</h2>
                            <p className="text-[11px] text-gray-500 mt-0.5">Select a break policy to start</p>
                        </div>
                    </div>

                    {loading ? (
                        <div className="py-8 flex flex-col items-center justify-center text-gray-400">
                            <div className="w-5 h-5 border-2 border-gray-200 border-t-primary rounded-full animate-spin mb-2" />
                            <span className="text-[11px]">Loading policies...</span>
                        </div>
                    ) : error ? (
                        <div className="py-4 text-center">
                            <p className="text-red-500 text-xs font-medium">{error}</p>
                            <button onClick={onClose} className="mt-4 text-primary text-[11px] font-bold hover:underline underline-offset-4">Dismiss</button>
                        </div>
                    ) : policies.length === 0 ? (
                        <div className="py-6 text-center">
                            <p className="text-gray-500 text-xs">No break policies available for this project.</p>
                            <button onClick={onClose} className="mt-4 text-primary text-[11px] font-bold hover:underline underline-offset-4">Close</button>
                        </div>
                    ) : (
                        <div className="space-y-4">
                            <div className="space-y-1.5">
                                <label className="text-[10px] font-bold text-gray-400 uppercase tracking-widest px-1">Break Type</label>
                                <select
                                    value={selectedPolicyId}
                                    onChange={(e) => setSelectedPolicyId(e.target.value)}
                                    className="w-full bg-gray-50 border border-gray-200 rounded-lg px-3 py-2 text-sm text-gray-700 outline-none focus:border-primary/40 focus:ring-4 focus:ring-primary/5 transition-all appearance-none cursor-pointer"
                                    style={{ backgroundImage: 'url("data:image/svg+xml,%3csvg xmlns=\'http://www.w3.org/2000/svg\' fill=\'none\' viewBox=\'0 0 20 20\'%3e%3cpath stroke=\'%23a1a1aa\' stroke-linecap=\'round\' stroke-linejoin=\'round\' stroke-width=\'1.5\' d=\'M6 8l4 4 4-4\'/%3e%3c/svg%3e")', backgroundPosition: 'right 0.5rem center', backgroundRepeat: 'no-repeat', backgroundSize: '1.5em 1.5em', paddingRight: '2.5rem' }}
                                >
                                    {policies.map((p) => (
                                        <option key={p.id} value={p.id}>
                                            {p.name} ({p.duration}m{p.paid ? ', Paid' : ', Unpaid'})
                                        </option>
                                    ))}
                                </select>
                            </div>

                            <div className="flex gap-2.5 pt-2">
                                <button
                                    onClick={onClose}
                                    className="flex-1 px-4 py-2 rounded-lg text-sm font-semibold text-gray-600 hover:bg-gray-50 transition-colors"
                                >
                                    Cancel
                                </button>
                                <button
                                    onClick={handleStartBreak}
                                    disabled={starting}
                                    className="flex-1 bg-primary text-white px-4 py-2 rounded-lg text-sm font-bold shadow-md hover:opacity-90 active:scale-95 transition-all disabled:opacity-50 disabled:active:scale-100"
                                >
                                    {starting ? "Starting..." : "Start Break"}
                                </button>
                            </div>
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}
