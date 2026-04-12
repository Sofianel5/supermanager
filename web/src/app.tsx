import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { SupermanagerAuthProvider } from "./auth";
import { LandingPage } from "./routes/landing-page";
import { InvitePage } from "./routes/invite-page";
import { LoginPage } from "./routes/login-page";
import { RoomPage } from "./routes/room-page";

export function App() {
  return (
    <BrowserRouter>
      <SupermanagerAuthProvider>
        <Routes>
          <Route path="/" element={<LandingPage />} />
          <Route path="/login" element={<LoginPage />} />
          <Route path="/invite/:token" element={<InvitePage />} />
          <Route path="/r/:roomId" element={<RoomPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </SupermanagerAuthProvider>
    </BrowserRouter>
  );
}
