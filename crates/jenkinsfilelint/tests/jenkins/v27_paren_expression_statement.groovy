pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    (env.A ?: 'x').trim()
                }
            }
        }
    }
}
