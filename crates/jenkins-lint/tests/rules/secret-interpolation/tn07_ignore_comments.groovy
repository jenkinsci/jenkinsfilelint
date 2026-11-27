pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
                    // jenkins-lint: ignore[secret-interpolation] the token is a fake one for tests
                    sh "deploy --token ${TOKEN}"
                    sh "deploy --token ${TOKEN}" // jenkins-lint: ignore[secret-interpolation]
                    // jenkins-lint: ignore[secret-interpolation]
                    sh """
                        deploy \\
                          --token ${TOKEN}
                    """
                }
            }
        }
    }
}
