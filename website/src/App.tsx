import React from 'react';
import { Navbar } from './components/Navbar';
import { Hero } from './components/Hero';
import { ProblemSection } from './components/ProblemSection';
import { HowItWorks } from './components/HowItWorks';
import { ProofSection } from './components/ProofSection';
import { PrivacySection } from './components/PrivacySection';
import { DownloadSection } from './components/DownloadSection';
import { FAQSection } from './components/FAQSection';
import { Footer } from './components/Footer';

export const App: React.FC = () => {
  return (
    <div className="min-h-screen bg-canvas text-ink-primary flex flex-col font-sans selection:bg-accent/15 selection:text-ink-primary">
      <Navbar />
      <main className="flex-grow">
        <Hero />
        <ProblemSection />
        <HowItWorks />
        <ProofSection />
        <PrivacySection />
        <DownloadSection />
        <FAQSection />
      </main>
      <Footer />
    </div>
  );
};

export default App;
