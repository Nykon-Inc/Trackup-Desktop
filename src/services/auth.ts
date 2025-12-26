import axios from 'axios';

const API_URL = import.meta.env.VITE_API_URL || 'http://localhost:8000';

const api = axios.create({
    baseURL: API_URL,
});

export interface User {
    id: string;
    name: string;
    email: string;
    createdAt: string;
    updatedAt: string;
}

export interface Project {
    id: string;
    name: string;
    organizationId: string;
    createdAt: string;
    updatedAt: string;
}

interface LoginResponse {
    account: User;
    credentials: {
        access: {
            token: string;
            expires: string;
        };
        refresh: {
            token: string;
            expires: string;
        };
    };
}

export const authenticateUser = async (email: string, password: string) => {
    // 1. Call to base_url/v1/auth/login
    const authResponse = await api.post<LoginResponse>('/v1/auth/login', {
        email,
        password
    });

    const { account, credentials } = authResponse.data;
    const token = credentials.access.token;

    // 2. Call to base_url/v1/projects using the token
    const projects = await fetchProjects(token);
    const project = projects[0];

    // 3. Construct the payload
    // We combine the user data, the token, and the projects list.
    // We also map 'id' to 'uuid' if the backend expects it, but we'll keep the spread clean.
    const payload = {
        name: account.name,
        email: account.email,
        token,
        projects: projects.map(e => ({ name: e.name, id: e.id })),
        // We can add a derived field for existing logic if needed, 
        // but for now we basically merge the info.
        uuid: account.id, // For compatibility with typical frontend usage if it expects uuid
        current_project_id: project?.id ?? null,
    };

    return payload;
};

export const fetchProjects = async (token: string) => {
    const projectsResponse = await api.get<{ results: Project[] }>('/v1/projects', {
        headers: {
            Authorization: `Bearer ${token}`
        }
    });
    return projectsResponse.data.results;
};
