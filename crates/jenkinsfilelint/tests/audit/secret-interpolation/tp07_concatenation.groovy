pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
                    sh 'deploy --token ' + TOKEN
                    sh('deploy --token ' + env.TOKEN)
                }
            }
        }
    }
}
