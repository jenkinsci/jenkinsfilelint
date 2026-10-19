pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    echo(env.A
                        + env.B)
                }
            }
        }
    }
}
