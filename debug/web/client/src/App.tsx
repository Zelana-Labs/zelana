import { BrowserRouter, Routes, Route } from "react-router-dom";
import Layout from "./components/Layout";
import Dashboard from "./pages/Dashboard";
import Accounts from "./pages/Accounts";
import Transactions from "./pages/Transactions";
import Batches from "./pages/Batches";
import Blocks from "./pages/Blocks";
import Shielded from "./pages/Shielded";
import Bridge from "./pages/Bridge";

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Layout />}>
          <Route index element={<Dashboard />} />
          <Route path="accounts" element={<Accounts />} />
          <Route path="transactions" element={<Transactions />} />
          <Route path="batches" element={<Batches />} />
          <Route path="blocks" element={<Blocks />} />
          <Route path="shielded" element={<Shielded />} />
          <Route path="bridge" element={<Bridge />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
