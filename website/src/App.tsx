import React from 'react';
import { Nav } from './components/Nav';
import { Hero } from './components/Hero';
import { Problem } from './components/Problem';
import { ProductStory } from './components/ProductStory';
import { Demo } from './components/Demo';
import { Numbers } from './components/Numbers';
import { Privacy } from './components/Privacy';
import { Install } from './components/Install';
import { Faq } from './components/Faq';
import { Footer } from './components/Footer';
import { usePageMotion } from './lib/motion';

export const App: React.FC = () => {
  usePageMotion();

  return (
    <div className="flex min-h-screen flex-col bg-paper font-sans text-ink">
      <a href="#main" className="skip-link">
        Skip to content
      </a>
      <Nav />
      <main id="main" tabIndex={-1} className="flex-grow focus:outline-none">
        <Hero />
        <Problem />
        <ProductStory />
        <Demo />
        <Numbers />
        <Privacy />
        <Install />
        <Faq />
      </main>
      <Footer />
    </div>
  );
};

export default App;
