import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { SupermanagerAuthProvider } from "./auth";
import { LandingPage } from "./routes/landing-page";
import { LoginPage } from "./routes/login-page";
import { RoomPage } from "./routes/room-page";

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<LandingPage />} />
        <Route
          path="/login"
          element={
            <SupermanagerAuthProvider>
              <LoginPage />
            </SupermanagerAuthProvider>
          }
        />
        <Route
          path="/r/:roomId"
          element={
            <SupermanagerAuthProvider>
              <RoomPage />
            </SupermanagerAuthProvider>
          }
        />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}
