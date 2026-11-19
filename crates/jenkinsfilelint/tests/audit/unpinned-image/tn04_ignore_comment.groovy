pipeline {
    // jenkinsfilelint: ignore[unpinned-image] a nightly job that tests the newest image
    agent { docker { image 'node:latest' } }
    stages {
        stage('Build') {
            steps {
                sh 'npm ci'
            }
        }
    }
}
