def run = {
    pipeline {
        agent any
        stages {
            stage('S') {
                steps { echo 'x' }
            }
        }
    }
}
try {
    run()
} finally {
    echo 'done'
}
