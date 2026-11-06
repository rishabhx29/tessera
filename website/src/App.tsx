import React from 'react';
import { Navbar } from './components/Navbar';
import { Hero } from './components/Hero';
import { ProblemSection } from './components/ProblemSection';
import { DemoSection } from './components/DemoSection';
import { HowItWorks } from './components/HowItWorks';
import { ProofSection } from './components/ProofSection';
import { PrivacySection } from './components/PrivacySection';
import { DownloadSection } from './components/DownloadSection';
import { FAQSection } from './components/FAQSection';
import { Footer } from './components/Footer';

export const App: React.FC = () => {
  return (
    <div
      className="flex min-h-screen flex-col bg-canvas font-sans text-ink-primary selection:bg-accent/15 selection:text-ink-primary"
    >
      <a href="#main" className="skip-link">
        Skip to content
      </a>
      <Navbar />
      <main id="main" tabIndex={-1} className="flex-grow focus:outline-none">
        <Hero />
        <ProblemSection />
        <DemoSection />
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
