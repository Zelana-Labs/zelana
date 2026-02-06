import type {ReactNode} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import HomepageFeatures from '@site/src/components/HomepageFeatures';
import Heading from '@theme/Heading';

import styles from './index.module.css';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <Heading as="h1" className={clsx('hero__title', styles.heroTitle)}>
          {siteConfig.title}
        </Heading>
        <p className={clsx('hero__subtitle', styles.heroSubtitle)}>
          {siteConfig.tagline}
        </p>
        <div className={styles.buttons}>
          <Link
            className={clsx('button button--secondary button--lg', styles.primaryCta)}
            to="/docs/intro">
            Get started
          </Link>
          <Link
            className={clsx('button button--outline button--lg', styles.secondaryCta)}
            to="/docs/implementation/architecture">
            Architecture
          </Link>
        </div>
        <p className={styles.heroNote}>
          Build, verify, and reason about the stack with implementation notes,
          state machines, and protocol details.
        </p>
      </div>
    </header>
  );
}

export default function Home(): ReactNode {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title="Zelana Documentation"
      description="Technical documentation for the Zelana rollup: architecture, state machines, and protocol details.">
      <HomepageHeader />
      <main>
        <HomepageFeatures />
      </main>
    </Layout>
  );
}
