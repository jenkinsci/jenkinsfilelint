pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    switch (env.X) {
                        case 'a':
                            echo 'a' +
                        default:
                            echo )
                    }
                }
            }
        }
    }
}
