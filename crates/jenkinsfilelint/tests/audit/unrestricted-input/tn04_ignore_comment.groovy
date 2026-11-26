pipeline {
    agent { label 'linux' }
    stages {
        stage('Confirm') {
            steps {
                // jenkins-lint: ignore[unrestricted-input] a confirmation, not a security gate
                input 'Continue with the slow tests?'
                sh 'make slow-test'
            }
        }
    }
}
