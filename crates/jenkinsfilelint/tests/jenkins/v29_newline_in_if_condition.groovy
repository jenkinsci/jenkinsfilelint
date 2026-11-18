pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    if (env.A == 'x'
                        || env.B == 'y') {
                        echo 'a'
                    }
                }
            }
        }
    }
}
