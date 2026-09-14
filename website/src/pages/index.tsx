import type {ReactNode} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useBaseUrl from '@docusaurus/useBaseUrl';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';
import FeatureHighlights from '@site/src/components/FeatureHighlights';
import ScreenshotStrip from '@site/src/components/ScreenshotStrip';
import Reveal from '@site/src/components/Reveal';

import styles from './index.module.css';

function HomepageHeader() {
  const heroShot = useBaseUrl('/00-overview.png');
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <p className={styles.brandMark}>Atlas</p>
        <Heading as="h1" className="hero__title">
          Storage, as a product.
        </Heading>
        <p className="hero__subtitle">
          Zyvor&apos;s control plane maps intent to Ceph, NFS, and ZFS — with an
          Apple Shop console for operators.
        </p>
        <div className={styles.buttons}>
          <Link
            className={clsx('button button--secondary button--lg', styles.pill)}
            to="/docs/getting-started/quickstart">
            Get Started
          </Link>
          <Link
            className={clsx(
              'button button--outline button--lg button--secondary',
              styles.pill,
            )}
            to="https://github.com/zyvorai/atlas">
            View on GitHub
          </Link>
        </div>
        <div className={styles.heroMedia}>
          <img
            src={heroShot}
            alt="Atlas Storage Center — Overview"
            onError={(e) => {
              (e.currentTarget as HTMLImageElement).style.display = 'none';
            }}
          />
        </div>
      </div>
    </header>
  );
}

function ProblemStatement() {
  return (
    <section className={styles.problem}>
      <div className="container">
        <Reveal className="text--center">
          <Heading as="h2" className={styles.sectionHeading}>
            Intent in. Backend details out.
          </Heading>
          <p className={styles.lede}>
            Products request production block storage — not pool names or CSI
            quirks. Atlas owns inventory, ownership, audit, and the async job
            engine so the suite stays decoupled from Ceph.
          </p>
        </Reveal>
      </div>
    </section>
  );
}

function TrustBand() {
  return (
    <section className={styles.trust}>
      <div className="container">
        <Reveal className={styles.trustGrid}>
          <div>
            <Heading as="h3" className={styles.sectionHeading}>
              Dual-licensed, operator-honest
            </Heading>
            <p className={styles.lede}>
              AGPL-3.0 for home and self-host. Atlas Commercial License when you
              need freedom from copyleft, proprietary integrations, or support.
            </p>
            <Link to="/docs/licensing">Read the licensing guide →</Link>
          </div>
          <div className={styles.trustBadges}>
            <img
              src="https://github.com/zyvorai/atlas/actions/workflows/ci.yml/badge.svg"
              alt="CI status"
            />
            <img
              src="https://img.shields.io/badge/License-AGPL%20v3-blue.svg"
              alt="AGPL v3 license"
            />
          </div>
        </Reveal>
      </div>
    </section>
  );
}

function EnterpriseCTA() {
  return (
    <section className={styles.enterprise}>
      <div className="container text--center">
        <Reveal>
          <Heading as="h2" className={styles.sectionHeading}>
            Need ACL, support, or SLAs?
          </Heading>
          <p className={styles.enterpriseCopy}>
            Free under AGPL for home and internal self-host. Buy an Atlas
            Commercial License for proprietary integrations, warranties, or
            ongoing support.
          </p>
          <Link
            className={clsx('button button--primary button--lg', styles.pill)}
            to="mailto:sales@zyvor.dev">
            Contact sales@zyvor.dev
          </Link>
        </Reveal>
      </div>
    </section>
  );
}

export default function Home(): ReactNode {
  return (
    <Layout
      title="Atlas — Zyvor storage control plane"
      description="Central storage control plane for the Zyvor suite — Ceph-first, pluggable drivers, Apple Shop console.">
      <HomepageHeader />
      <main>
        <ProblemStatement />
        <Reveal>
          <FeatureHighlights />
        </Reveal>
        <Reveal>
          <ScreenshotStrip />
        </Reveal>
        <TrustBand />
        <EnterpriseCTA />
      </main>
    </Layout>
  );
}
