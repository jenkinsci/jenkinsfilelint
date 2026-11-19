pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
                    // jenkinsfilelint: ignore[secret-interpolation] the token is a fake one for tests
                    sh "deploy --token ${TOKEN}"
                    sh "deploy --token ${TOKEN}" // jenkinsfilelint: ignore[secret-interpolation]
                    // jenkinsfilelint: ignore[secret-interpolation]
                    sh """
                        deploy \\
                          --token ${TOKEN}
                    """
                }
            }
        }
    }
}
