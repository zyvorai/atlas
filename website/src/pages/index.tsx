// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
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
              Free to evaluate. Licensed for production.
            </Heading>
            <p className={styles.lede}>
              The Zyvor Production License covers development, testing, and
              non-production labs. Production, customer workloads, and
              revenue-generating services need a paid commercial license.
            </p>
            <Link to="/docs/licensing">Read the licensing guide →</Link>
          </div>
          <div className={styles.trustBadges}>
            <img
              src="https://github.com/zyvorai/atlas/actions/workflows/ci.yml/badge.svg"
              alt="CI status"
            />
            <img
              src="https://img.shields.io/badge/License-Zyvor%20Production%20v1.0-blue.svg"
              alt="Zyvor Production License v1.0"
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
            Production use needs a commercial license
          </Heading>
          <p className={styles.enterpriseCopy}>
            Evaluation, development, and laboratory use are free. Production
            deployments, SaaS, managed services, and OEM use are licensed
            separately by Zyvor.
          </p>
          <Link
            className={clsx('button button--primary button--lg', styles.pill)}
            to="https://zyvor.dev">
            zyvor.dev
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
